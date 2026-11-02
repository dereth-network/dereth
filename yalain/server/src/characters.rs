//! The operator's character files: a build's recipe, read from TOML, and a character carried
//! between worlds, written to and read from JSON. What they mean is `yalain_world::characters`'s.
//!
//! A recipe:
//!
//! ```toml
//! account = "builder"
//! name = "Hand Of Doom"
//! race = 1            # 1 human, 2 lugian, 4 tumerok, 8 drudge, 16 empyrean
//! sex = 4096          # 4096 or 8192
//! level = 20          # or experience = 250000
//! money = 5000
//! hero = true
//! place = { cell = 0x77B80006, x = 4.4, y = 100.8, z = 182.5, heading = 90 }
//! skills = [{ skill = 15, rating = 10 }]
//! items = [{ template = 0x4700051C, quantity = 1 }]
//! quests = [{ quest = 0x800000E0, phase = "done" }, { quest = 0x800000E1, phase = 2 }]
//! ```
//!
//! A character file is JSON: its format and version, the world it left, the character's record
//! and what it is in the world, the server's own encodings as hexadecimal text, and its build: what
//! it holds in a recipe's own words (`yalain_world::characters::build_of`), for people and world
//! builders to read, with no account. Bringing a character in reads the record and the state, not
//! the build. A file is written new, never over another.
//!
//! ```json
//! "build": {
//!   "name": "Hand Of Doom", "race": 4, "sex": 8192, "level": 20, "money": 5000, "hero": true,
//!   "place": { "cell": 2008547334, "x": 4.4, "y": 100.8, "z": 182.5, "heading": 90.0 },
//!   "skills": [{ "skill": 15, "rating": 10 }],
//!   "items": [{ "template": 1191183644, "quantity": 1 }],
//!   "quests": [{ "quest": 2147483872, "phase": "done" }, { "quest": 2147483873, "phase": 2 }]
//! }
//! ```

use std::path::Path;

use serde::{Deserialize, Serialize};
use yalain_store::{Character, StoredPosition, WorldState};
use yalain_world::characters::{CharacterFile, QuestAt, Recipe};

/// What a character file says it is.
const FORMAT: &str = "yalain-character";
/// The character file's version: 2 carries the build.
const VERSION: u32 = 2;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecipeToml {
    account: String,
    name: String,
    race: u32,
    sex: u32,
    level: Option<i32>,
    experience: Option<u64>,
    money: Option<u32>,
    hero: Option<bool>,
    place: Option<PlaceToml>,
    #[serde(default)]
    skills: Vec<SkillToml>,
    #[serde(default)]
    items: Vec<ItemToml>,
    #[serde(default)]
    quests: Vec<QuestToml>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PlaceToml {
    cell: u32,
    x: f32,
    y: f32,
    z: f32,
    #[serde(default)]
    heading: f32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SkillToml {
    skill: u32,
    rating: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemToml {
    template: u32,
    #[serde(default = "one")]
    quantity: u32,
}

const fn one() -> u32 {
    1
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestToml {
    quest: u32,
    phase: PhaseToml,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum PhaseToml {
    Number(u32),
    Word(String),
}

/// A recipe from its text.
///
/// # Errors
/// It does not read, or a quest's phase is neither a number nor `done`.
pub fn recipe_from(text: &str) -> Result<Recipe, String> {
    let r: RecipeToml = toml::from_str(text).map_err(|e| e.to_string())?;
    let b = BuildJson {
        name: r.name,
        race: r.race,
        sex: r.sex,
        level: r.level,
        experience: r.experience,
        money: r.money,
        hero: r.hero,
        place: r.place,
        skills: r.skills,
        items: r.items,
        quests: r.quests,
    };
    Ok(Recipe {
        account: r.account,
        ..b.recipe()?
    })
}

/// A build as a character file holds it: a recipe's fields, with no account.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildJson {
    name: String,
    race: u32,
    sex: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    level: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    experience: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    money: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hero: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    place: Option<PlaceToml>,
    #[serde(default)]
    skills: Vec<SkillToml>,
    #[serde(default)]
    items: Vec<ItemToml>,
    #[serde(default)]
    quests: Vec<QuestToml>,
}

impl BuildJson {
    /// The build a recipe holds, with no account.
    fn of(r: &Recipe) -> Self {
        Self {
            name: r.name.clone(),
            race: r.race,
            sex: r.sex,
            level: r.level,
            experience: r.experience,
            money: r.money,
            hero: r.hero,
            place: r.place.map(|p| PlaceToml {
                cell: p.cell,
                x: p.origin[0],
                y: p.origin[1],
                z: p.origin[2],
                heading: degrees_of_rotation(p.rotation),
            }),
            skills: r
                .skills
                .iter()
                .map(|&(skill, rating)| SkillToml { skill, rating })
                .collect(),
            items: r
                .items
                .iter()
                .map(|&(template, quantity)| ItemToml { template, quantity })
                .collect(),
            quests: r
                .quests
                .iter()
                .map(|&(quest, at)| QuestToml {
                    quest,
                    phase: match at {
                        QuestAt::Done => PhaseToml::Word("done".into()),
                        QuestAt::Phase(n) => PhaseToml::Number(n),
                    },
                })
                .collect(),
        }
    }

    /// The recipe it says, with no account.
    fn recipe(self) -> Result<Recipe, String> {
        let quests = self
            .quests
            .into_iter()
            .map(|q| match q.phase {
                PhaseToml::Number(n) => Ok((q.quest, QuestAt::Phase(n))),
                PhaseToml::Word(w) if w.eq_ignore_ascii_case("done") => {
                    Ok((q.quest, QuestAt::Done))
                }
                PhaseToml::Word(w) => Err(format!(
                    "quest {:08X}: the phase is a number or \"done\", not {w:?}",
                    q.quest
                )),
            })
            .collect::<Result<_, _>>()?;
        Ok(Recipe {
            account: String::new(),
            name: self.name,
            race: self.race,
            sex: self.sex,
            level: self.level,
            experience: self.experience,
            money: self.money,
            hero: self.hero,
            skills: self
                .skills
                .into_iter()
                .map(|s| (s.skill, s.rating))
                .collect(),
            items: self
                .items
                .into_iter()
                .map(|i| (i.template, i.quantity))
                .collect(),
            quests,
            place: self.place.map(|p| StoredPosition {
                cell: p.cell,
                origin: [p.x, p.y, p.z],
                rotation: crate::console::rotation_of_degrees(p.heading),
            }),
        })
    }
}

/// A rotation's heading in degrees clockwise from north, 0 to 360, to a hundredth of a degree
/// (the inverse of [`crate::console::rotation_of_degrees`]).
fn degrees_of_rotation(r: [f32; 4]) -> f32 {
    let degrees =
        (-2.0 * dereth_primitives::num::math::atan2(f64::from(r[3]), f64::from(r[0]))).to_degrees();
    let hundredths = (degrees.rem_euclid(360.0) * 100.0).round() / 100.0;
    #[allow(clippy::cast_possible_truncation)]
    let heading = (hundredths % 360.0) as f32;
    heading
}

/// A recipe from its file.
///
/// # Errors
/// The file does not read, or its recipe does not.
pub fn read_recipe(path: &Path) -> Result<Recipe, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    recipe_from(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileJson {
    format: String,
    version: u32,
    world: String,
    character: CharacterJson,
    state: StateJson,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    build: Option<BuildJson>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterJson {
    id: u64,
    name: String,
    race: u32,
    sex: u32,
    class: u32,
    implement: u32,
    start_area: u32,
    entity_did: u32,
    physique: Vec<(u32, f32)>,
    look: String,
    created_at: i64,
    last_played: i64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateJson {
    position: Option<PositionJson>,
    instances: u16,
    qualities: String,
    options: String,
    places: Vec<(u32, PositionJson)>,
    play: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(deny_unknown_fields)]
struct PositionJson {
    cell: u32,
    origin: [f32; 3],
    rotation: [f32; 4],
}

impl From<StoredPosition> for PositionJson {
    fn from(p: StoredPosition) -> Self {
        Self {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }
    }
}

impl From<PositionJson> for StoredPosition {
    fn from(p: PositionJson) -> Self {
        Self {
            cell: p.cell,
            origin: p.origin,
            rotation: p.rotation,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err("an odd length of hexadecimal text".into());
    }
    (0..text.len())
        .step_by(2)
        .map(|i| {
            text.get(i..i + 2)
                .and_then(|d| u8::from_str_radix(d, 16).ok())
                .ok_or_else(|| "hexadecimal text that does not read".to_owned())
        })
        .collect()
}

/// A character file's text.
///
/// # Errors
/// It does not encode (never, for a character the store read).
pub fn file_text(f: &CharacterFile) -> Result<String, String> {
    let c = &f.character;
    let st = &f.state;
    let json = FileJson {
        format: FORMAT.into(),
        version: VERSION,
        world: f.world.clone(),
        character: CharacterJson {
            id: c.id,
            name: c.name.clone(),
            race: c.race,
            sex: c.sex,
            class: c.class,
            implement: c.implement,
            start_area: c.start_area,
            entity_did: c.entity_did,
            physique: c.physique.clone(),
            look: hex(&c.look),
            created_at: c.created_at,
            last_played: c.last_played,
        },
        state: StateJson {
            position: st.position.map(PositionJson::from),
            instances: st.instances,
            qualities: hex(&st.qualities),
            options: hex(&st.options),
            places: st
                .places
                .iter()
                .map(|(k, p)| (*k, PositionJson::from(*p)))
                .collect(),
            play: hex(&st.play),
        },
        build: f.build.as_ref().map(BuildJson::of),
    };
    serde_json::to_string_pretty(&json).map_err(|e| e.to_string())
}

/// A character file from its text.
///
/// # Errors
/// It does not read, is not a character file, or a newer Yalain wrote it.
pub fn file_from(text: &str) -> Result<CharacterFile, String> {
    let j: FileJson = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if j.format != FORMAT {
        return Err("not a Yalain character file".into());
    }
    if j.version > VERSION {
        return Err(format!(
            "a newer Yalain wrote it (version {}; this one reads {VERSION})",
            j.version
        ));
    }
    let c = j.character;
    Ok(CharacterFile {
        build: j.build.map(BuildJson::recipe).transpose()?,
        world: j.world,
        character: Character {
            id: c.id,
            account: 0,
            name: c.name,
            race: c.race,
            sex: c.sex,
            class: c.class,
            implement: c.implement,
            start_area: c.start_area,
            entity_did: c.entity_did,
            physique: c.physique,
            look: unhex(&c.look)?,
            created_at: c.created_at,
            deleted_at: None,
            last_played: c.last_played,
        },
        state: WorldState {
            position: j.state.position.map(StoredPosition::from),
            instances: j.state.instances,
            qualities: unhex(&j.state.qualities)?,
            options: unhex(&j.state.options)?,
            places: j
                .state
                .places
                .into_iter()
                .map(|(k, p)| (k, StoredPosition::from(p)))
                .collect(),
            play: unhex(&j.state.play)?,
        },
    })
}

/// Write a character to a new file.
///
/// # Errors
/// A file is there already, or it does not write.
pub fn write_file(path: &Path, f: &CharacterFile) -> Result<(), String> {
    use std::io::Write as _;
    let text = file_text(f)?;
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    out.write_all(text.as_bytes()).map_err(|e| {
        // Nothing is left behind of a file that did not write.
        drop(out);
        let _ = std::fs::remove_file(path);
        format!("{}: {e}", path.display())
    })
}

/// Read a character file.
///
/// # Errors
/// It does not read, or is no character file this Yalain reads.
pub fn read_file(path: &Path) -> Result<CharacterFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file_from(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recipe file reads into its build: the account and name, race and sex, level, money, hero,
    /// place, skills, items (one each unless a quantity is given) and quests (done or at a phase);
    /// a field it does not know, or a phase that is neither, is refused.
    /// Behaviour: server.operator.characters-made-with-a-build
    #[test]
    fn a_recipe_file_reads_into_the_build_it_describes() {
        let r = recipe_from(
            r#"
            account = "builder"
            name = "Hand Of Doom"
            race = 4
            sex = 8192
            level = 20
            money = 5000
            hero = true
            place = { cell = 0x77B80006, x = 4.4, y = 100.8, z = 182.5 }
            skills = [{ skill = 15, rating = 10 }]
            items = [{ template = 0x4700051C }, { template = 0x47002108, quantity = 5 }]
            quests = [{ quest = 0x800000E0, phase = "done" }, { quest = 0x800000E1, phase = 2 }]
            "#,
        )
        .expect("reads");
        assert_eq!(
            (r.account.as_str(), r.name.as_str()),
            ("builder", "Hand Of Doom")
        );
        assert_eq!(
            (r.race, r.sex, r.level, r.experience),
            (4, 8192, Some(20), None)
        );
        assert_eq!((r.money, r.hero), (Some(5000), Some(true)));
        assert_eq!(r.skills, [(15, 10)]);
        assert_eq!(r.items, [(0x4700_051C, 1), (0x4700_2108, 5)]);
        assert_eq!(
            r.quests,
            [
                (0x8000_00E0, QuestAt::Done),
                (0x8000_00E1, QuestAt::Phase(2))
            ]
        );
        let at = r.place.expect("a place");
        assert_eq!((at.cell, at.origin), (0x77B8_0006, [4.4, 100.8, 182.5]));
        assert_eq!(at.rotation, [1.0, 0.0, 0.0, 0.0], "facing north");
        let base = "account = \"a\"\nname = \"Kara\"\nrace = 1\nsex = 4096\n";
        assert!(recipe_from(&format!("{base}levle = 3\n")).is_err());
        assert!(recipe_from(&format!(
            "{base}quests = [{{ quest = 1, phase = \"soon\" }}]\n"
        ))
        .is_err());
        assert!(recipe_from(base).is_ok());
    }

    /// A character file holds the character's record and what it is in the world, and reads back
    /// as it was written; one that says another format, or a newer version, is refused.
    /// Behaviour: server.operator.characters-exported-and-imported
    #[test]
    fn a_character_file_reads_back_as_it_was_written() {
        let at = StoredPosition {
            cell: 0x77B8_0006,
            origin: [4.5, 100.25, 182.0],
            rotation: [0.5, 0.0, 0.0, -0.75],
        };
        let f = CharacterFile {
            world: "Builder's World".into(),
            character: Character {
                id: 0x2130_0000_0000_0007,
                account: 0,
                name: "Hand Of Doom".into(),
                race: 4,
                sex: 0x2000,
                class: 4,
                implement: 8,
                start_area: 0x4000_0005,
                entity_did: 0x4700_110F,
                physique: vec![(1, 0.94), (2, 1.01)],
                look: vec![0x22, 1, 0, 0, 0x7A, 0, 0, 0x1F],
                created_at: 1_484_464_962,
                deleted_at: None,
                last_played: 1_484_465_000,
            },
            state: WorldState {
                position: Some(at),
                instances: 12,
                qualities: vec![1, 2, 3, 0xFF],
                options: vec![9, 8],
                places: vec![(0x4000_0001, at)],
                play: vec![10, 0, 0, 0, 0xAB],
            },
            build: None,
        };
        let text = file_text(&f).expect("written");
        assert_eq!(file_from(&text).expect("reads"), f);
        let other = text.replace("\"yalain-character\"", "\"something-else\"");
        assert!(file_from(&other).is_err());
        let newer = text.replace("\"version\": 2", "\"version\": 3");
        assert!(file_from(&newer).is_err());
        let older = text.replace("\"version\": 2", "\"version\": 1");
        assert_eq!(file_from(&older).expect("a first file reads"), f);
    }

    /// A character file carries its build in a recipe's own words, readable without the server's
    /// encodings: its level, money and hero, each trained skill with its rating, its items, quests
    /// and place (the heading in degrees); the build reads back as it was written.
    /// Behaviour: server.operator.exported-characters-carry-their-build
    #[test]
    fn a_character_file_carries_its_build_in_a_recipes_words() {
        let build = recipe_from(
            r#"
            account = ""
            name = "Hand Of Doom"
            race = 4
            sex = 8192
            level = 20
            money = 5000
            hero = true
            place = { cell = 0x77B80006, x = 4.5, y = 100.25, z = 182.0, heading = 90 }
            skills = [{ skill = 15, rating = 10 }, { skill = 26, rating = 3 }]
            items = [{ template = 0x4700051C, quantity = 2 }]
            quests = [{ quest = 0x800000E0, phase = "done" }, { quest = 0x800000E1, phase = 2 }]
            "#,
        )
        .expect("a recipe");
        let f = CharacterFile {
            world: "Builder's World".into(),
            character: Character {
                id: 0x2130_0000_0000_0007,
                name: "Hand Of Doom".into(),
                account: 0,
                race: 4,
                sex: 0x2000,
                class: 4,
                implement: 8,
                start_area: 0x4000_0005,
                entity_did: 0x4700_110F,
                physique: Vec::new(),
                look: Vec::new(),
                created_at: 0,
                deleted_at: None,
                last_played: 0,
            },
            state: WorldState::default(),
            build: Some(build.clone()),
        };
        let text = file_text(&f).expect("written");
        let v: serde_json::Value = serde_json::from_str(&text).expect("JSON");
        let b = &v["build"];
        assert_eq!(b["level"], 20);
        assert_eq!(b["money"], 5000);
        assert_eq!(b["hero"], true);
        assert_eq!(
            b["skills"][0],
            serde_json::json!({ "skill": 15, "rating": 10 })
        );
        assert_eq!(
            b["skills"][1],
            serde_json::json!({ "skill": 26, "rating": 3 })
        );
        assert_eq!(
            b["items"][0],
            serde_json::json!({ "template": 0x4700_051C_u32, "quantity": 2 })
        );
        assert_eq!(b["quests"][0]["phase"], "done");
        assert_eq!(b["quests"][1]["phase"], 2);
        assert_eq!(b["place"]["heading"], 90.0);
        assert!(b.get("account").is_none(), "no account");
        assert!(b.get("experience").is_none(), "a level, not an experience");
        assert_eq!(file_from(&text).expect("reads").build, Some(build));
    }
}
