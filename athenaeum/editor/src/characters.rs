//! Playtest characters: recipes kept with the world, each a player to play the world as, made by
//! the editor's own Yalain.
//!
//! A recipe is `characters/<Name>.toml` beside `world.toml`. It says what the character is, what
//! it has reached and where it starts:
//!
//! ```toml
//! race = "human"      # human, lugian, tumerok, drudge or empyrean
//! sex = "female"      # male or female
//! start = "camera"    # camera, selection, left, or "place <key>"
//! hero = false        # a hero earns focus, its third vital
//! level = 20          # or experience = 250000
//! money = 5000
//! skills = [{ skill = 15, rating = 10 }]
//! items = [{ template = 0x4700051C, quantity = 1 }]
//! quests = [{ quest = 0x800000E0, phase = "done" }, { quest = 0x800000E1, phase = 2 }]
//! ```
//!
//! Only the name (the file's) is needed: a recipe that says nothing more is a level-1 human man at
//! the camera. The build (the level or the experience, money, skills, items and quests) is made as
//! a player reaching it would hold it: the server trains each skill with the credits the levels
//! give and raises it with the experience, puts the items (templates the world serves) in the
//! pack, and the quests done or under way at a phase; a build that does not hold is refused, and
//! the character is not made.
//!
//! `start` is where Play puts it: on the ground under the camera, facing as the camera faces; in
//! front of the selected row, facing it; where it stood when it last left the world; or at one of
//! the world's places. The world database reads only its packs' folders, so the recipes are never
//! part of the world the server serves.
//!
//! The character itself lives in the playtest folder's one store, on an account of its own
//! ([`crate::live::account_of`]): the server makes it there from its recipe (written as the
//! server's own, [`Recipe::server_text`]) the first time it plays, and keeps what it finds, learns
//! and earns there after. In `.playtest/characters/<Name>/`, `made.toml` is the recipe as it was
//! last made or given ([`making`]). A recipe whose build changed since is given what changed
//! ([`Recipe::changes_text`]), keeping what the character found and learned: its level and skills
//! are never lowered, nor items taken away. One whose body (race or sex) changed is made anew. Its start and whether it is a
//! hero never remake it. Reset makes it anew from its recipe.
//!
//! A character brought in from another world's file comes with a recipe of what it holds, read
//! from the build the file gives in a recipe's own words ([`exported`]): its level, money and
//! hero, the skills it trained with their ratings, its items and its quests, starting where it
//! stood.

use std::path::{Path, PathBuf};

use auberean_protocol::types::Qualities;
use auberean_protocol::wire::{Field as _, Reader};
use auberean_world::chargen;

use crate::live::{account_of, STORE};

/// The folder of the recipes, in the world's folder.
pub const FOLDER: &str = "characters";

/// The races a recipe may name, as it names them and as the server takes them.
pub const RACES: [(&str, u32); 5] = [
    ("human", chargen::HUMAN),
    ("lugian", chargen::LUGIAN),
    ("tumerok", chargen::TUMEROK),
    ("drudge", chargen::DRUDGE),
    ("empyrean", chargen::EMPYREAN),
];

/// The sexes, as a recipe names them and as the server takes them.
pub const SEXES: [(&str, u32); 2] = [("male", chargen::MALE), ("female", chargen::FEMALE)];

/// Where a character starts when it plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// On the ground under the camera, facing as it faces.
    Camera,
    /// In front of the selected row, facing it.
    Selection,
    /// Where it stood when it last left the world (the camera's place the first time).
    Left,
    /// At a place of the world, by its key.
    Place(String),
}

impl Start {
    /// As a recipe writes it.
    #[must_use]
    pub fn word(&self) -> String {
        match self {
            Self::Camera => "camera".into(),
            Self::Selection => "selection".into(),
            Self::Left => "left".into(),
            Self::Place(k) => format!("place {k}"),
        }
    }

    /// From a recipe's word.
    ///
    /// # Errors
    /// A word that names no start.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim() {
            "camera" => Ok(Self::Camera),
            "selection" => Ok(Self::Selection),
            "left" => Ok(Self::Left),
            other => match other.strip_prefix("place ") {
                Some(k) if !k.trim().is_empty() => Ok(Self::Place(k.trim().to_string())),
                _ => Err(format!(
                    "start: {other:?} is none of camera, selection, left or \"place <key>\""
                )),
            },
        }
    }
}

/// A skill a recipe trains, raised to a rating.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Skill {
    pub skill: u32,
    pub rating: u32,
}

/// An item in a recipe's pack: a template the world serves, and how many.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Item {
    pub template: u32,
    pub quantity: u32,
}

/// Where a recipe's quest stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestAt {
    /// Completed.
    Done,
    /// Under way, at this phase (1 or more).
    Phase(u32),
}

/// A quest a recipe has done or has under way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quest {
    pub quest: u32,
    pub at: QuestAt,
}

/// A playtest character's recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub name: String,
    /// The race and sex, as the server takes them.
    pub race: u32,
    pub sex: u32,
    pub start: Start,
    pub hero: bool,
    /// Its level (its experience the level's own), or its total experience; at most one.
    pub level: Option<i32>,
    pub experience: Option<u64>,
    pub money: Option<u32>,
    pub skills: Vec<Skill>,
    pub items: Vec<Item>,
    pub quests: Vec<Quest>,
}

/// A race's or sex's word.
fn word_of(table: &[(&'static str, u32)], v: u32) -> &'static str {
    table.iter().find(|(_, x)| *x == v).map_or("?", |(w, _)| *w)
}

/// A race's or sex's value.
fn value_of(table: &[(&str, u32)], what: &str, w: &str) -> Result<u32, String> {
    table
        .iter()
        .find(|(x, _)| x.eq_ignore_ascii_case(w.trim()))
        .map(|(_, v)| *v)
        .ok_or_else(|| {
            let all: Vec<&str> = table.iter().map(|(x, _)| *x).collect();
            format!("{what}: {w:?} is none of {}", all.join(", "))
        })
}

/// Whether `name` is a name the server makes a character of: two to sixteen English letters,
/// spaces, apostrophes and hyphens, beginning and ending with a letter, never two spaces together.
///
/// # Errors
/// Why it is not one.
pub fn check_name(name: &str) -> Result<(), String> {
    let n = name.chars().count();
    if !(2..=16).contains(&n) {
        return Err(format!("{name:?}: a name is 2 to 16 letters long"));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphabetic() || matches!(c, ' ' | '\'' | '-'))
    {
        return Err(format!(
            "{name:?}: a name holds English letters, spaces, apostrophes and hyphens"
        ));
    }
    let letter = |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphabetic());
    if !letter(name.chars().next()) || !letter(name.chars().last()) {
        return Err(format!("{name:?}: a name begins and ends with a letter"));
    }
    if name.contains("  ") {
        return Err(format!("{name:?}: a name never has two spaces together"));
    }
    Ok(())
}

/// A whole number of a recipe, in the range `T` holds and from `least` on.
fn whole<T: TryFrom<i64>>(v: &toml::Value, what: &str, least: i64) -> Result<T, String> {
    v.as_integer()
        .filter(|n| *n >= least)
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| format!("{what}: a whole number from {least} on"))
}

/// The tables of a recipe's list `key` (none when it has none), each holding only `allowed`
/// fields and every one of `needed`.
fn tables<'a>(
    t: &'a toml::Table,
    key: &str,
    allowed: &[&str],
    needed: &[&str],
) -> Result<Vec<&'a toml::Table>, String> {
    let Some(v) = t.get(key) else {
        return Ok(Vec::new());
    };
    let bad = || format!("{key}: a list of tables");
    let mut out = Vec::new();
    for x in v.as_array().ok_or_else(bad)? {
        let x = x.as_table().ok_or_else(bad)?;
        if let Some(k) = x.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!("{key}: {k} is not one of {}", allowed.join(", ")));
        }
        if let Some(k) = needed.iter().find(|k| !x.contains_key(**k)) {
            return Err(format!("{key}: each needs a {k}"));
        }
        out.push(x);
    }
    Ok(out)
}

/// A list's entries as a recipe writes them, inline.
fn inline(entries: impl Iterator<Item = String>) -> String {
    format!("[{}]", entries.collect::<Vec<_>>().join(", "))
}

impl QuestAt {
    /// As a recipe writes it.
    fn text(self) -> String {
        match self {
            Self::Done => "\"done\"".into(),
            Self::Phase(n) => n.to_string(),
        }
    }
}

/// A build, or the part of one a server's recipe gives.
struct Build<'a> {
    level: Option<i32>,
    experience: Option<u64>,
    money: Option<u32>,
    skills: &'a [Skill],
    items: &'a [Item],
    quests: &'a [Quest],
}

/// A build's lines, as a recipe and the server's recipe both write them.
fn build_lines(b: &Build<'_>) -> String {
    let mut t = String::new();
    if let Some(l) = b.level {
        t += &format!("level = {l}\n");
    }
    if let Some(x) = b.experience {
        t += &format!("experience = {x}\n");
    }
    if let Some(m) = b.money {
        t += &format!("money = {m}\n");
    }
    if !b.skills.is_empty() {
        let s = inline(
            b.skills
                .iter()
                .map(|s| format!("{{ skill = {}, rating = {} }}", s.skill, s.rating)),
        );
        t += &format!("skills = {s}\n");
    }
    if !b.items.is_empty() {
        let s = inline(b.items.iter().map(|i| {
            format!(
                "{{ template = 0x{:08X}, quantity = {} }}",
                i.template, i.quantity
            )
        }));
        t += &format!("items = {s}\n");
    }
    if !b.quests.is_empty() {
        let s = inline(
            b.quests
                .iter()
                .map(|q| format!("{{ quest = 0x{:08X}, phase = {} }}", q.quest, q.at.text())),
        );
        t += &format!("quests = {s}\n");
    }
    t
}

/// The fields a recipe may hold.
const FIELDS: [&str; 10] = [
    "race",
    "sex",
    "start",
    "hero",
    "level",
    "experience",
    "money",
    "skills",
    "items",
    "quests",
];

impl Recipe {
    /// A new recipe: a level-1 human man who starts at the camera.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            race: chargen::HUMAN,
            sex: chargen::MALE,
            start: Start::Camera,
            hero: false,
            level: None,
            experience: None,
            money: None,
            skills: Vec::new(),
            items: Vec::new(),
            quests: Vec::new(),
        }
    }

    /// The race's word.
    #[must_use]
    pub fn race_word(&self) -> &'static str {
        word_of(&RACES, self.race)
    }

    /// The sex's word.
    #[must_use]
    pub fn sex_word(&self) -> &'static str {
        word_of(&SEXES, self.sex)
    }

    /// The build's lines, as a recipe and the server's recipe both write them.
    fn build_text(&self) -> String {
        build_lines(&Build {
            level: self.level,
            experience: self.experience,
            money: self.money,
            skills: &self.skills,
            items: &self.items,
            quests: &self.quests,
        })
    }

    /// The recipe's text.
    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "# A playtest character: what Athenaeum's Play makes on its own Yalain.\n\
             race = \"{}\"\nsex = \"{}\"\nstart = \"{}\"\nhero = {}\n{}",
            self.race_word(),
            self.sex_word(),
            self.start.word(),
            self.hero,
            self.build_text()
        )
    }

    /// The server's recipe's first lines: the character on its own account, its body, a hero or
    /// not when `hero` says, and where it enters when a `place` is given.
    fn server_head(&self, hero: Option<bool>, place: Option<(u32, [f32; 3], f32)>) -> String {
        let mut t = format!(
            "# Written by Athenaeum from characters/{0}.toml.\naccount = \"{1}\"\n\
             name = \"{0}\"\nrace = {2}\nsex = {3}\n",
            self.name,
            account_of(&self.name),
            self.race,
            self.sex
        );
        if let Some(h) = hero {
            t += &format!("hero = {h}\n");
        }
        if let Some((cell, at, heading)) = place {
            t += &format!(
                "place = {{ cell = 0x{cell:08X}, x = {:?}, y = {:?}, z = {:?}, heading = {heading:?} }}\n",
                at[0], at[1], at[2]
            );
        }
        t
    }

    /// The recipe as the server's `make-character` reads it: on the character's own account,
    /// entering at `place` (a cell, a point in its landblock and a heading in degrees) when one is
    /// given.
    #[must_use]
    pub fn server_text(&self, place: Option<(u32, [f32; 3], f32)>) -> String {
        self.server_head(Some(self.hero), place) + &self.build_text()
    }

    /// What changed of the build since a character was `made`, as the server's `apply-build`
    /// reads it: the level or experience and the money when they changed, the skills, items and
    /// quests new or changed, a hero or not when that changed, and where it enters when a `place`
    /// is given. The server raises the character's level and skills to it (never lowering them),
    /// adds the items it lacks and sets the money and quests named; what the text leaves out (all
    /// that did not change) stays as the character has it.
    #[must_use]
    pub fn changes_text(&self, made: &Self, place: Option<(u32, [f32; 3], f32)>) -> String {
        let reached = (self.level, self.experience) != (made.level, made.experience);
        let skills: Vec<Skill> = self
            .skills
            .iter()
            .filter(|s| !made.skills.contains(s))
            .copied()
            .collect();
        let items: Vec<Item> = self
            .items
            .iter()
            .filter(|i| !made.items.contains(i))
            .copied()
            .collect();
        let quests: Vec<Quest> = self
            .quests
            .iter()
            .filter(|q| !made.quests.contains(q))
            .copied()
            .collect();
        let hero = (self.hero != made.hero).then_some(self.hero);
        self.server_head(hero, place)
            + &build_lines(&Build {
                level: self.level.filter(|_| reached),
                experience: self.experience.filter(|_| reached),
                money: self.money.filter(|_| self.money != made.money),
                skills: &skills,
                items: &items,
                quests: &quests,
            })
    }

    /// The recipe of `name` from its text.
    ///
    /// # Errors
    /// Text that is not TOML, or a field that is not one.
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        check_name(name)?;
        let t: toml::Table = text.parse().map_err(|e| format!("{name}: not TOML: {e}"))?;
        if let Some(k) = t.keys().find(|k| !FIELDS.contains(&k.as_str())) {
            return Err(format!("{name}: {k} is not a recipe's field"));
        }
        let field = |k: &str| -> Result<Option<&str>, String> {
            t.get(k)
                .map(|v| {
                    v.as_str()
                        .ok_or_else(|| format!("{name}: {k} is a word in quotes"))
                })
                .transpose()
        };
        let mut r = Self::new(name);
        if let Some(w) = field("race")? {
            r.race = value_of(&RACES, "race", w)?;
        }
        if let Some(w) = field("sex")? {
            r.sex = value_of(&SEXES, "sex", w)?;
        }
        if let Some(w) = field("start")? {
            r.start = Start::parse(w)?;
        }
        if let Some(v) = t.get("hero") {
            r.hero = v.as_bool().ok_or("hero: true or false")?;
        }
        r.level = t.get("level").map(|v| whole(v, "level", 1)).transpose()?;
        r.experience = t
            .get("experience")
            .map(|v| whole(v, "experience", 0))
            .transpose()?;
        if r.level.is_some() && r.experience.is_some() {
            return Err(format!("{name}: a level or an experience, not both"));
        }
        r.money = t.get("money").map(|v| whole(v, "money", 0)).transpose()?;
        for s in tables(&t, "skills", &["skill", "rating"], &["skill"])? {
            r.skills.push(Skill {
                skill: whole(&s["skill"], "skill", 1)?,
                rating: s.get("rating").map_or(Ok(0), |v| whole(v, "rating", 0))?,
            });
        }
        for i in tables(&t, "items", &["template", "quantity"], &["template"])? {
            r.items.push(Item {
                template: whole(&i["template"], "template", 1)?,
                quantity: i
                    .get("quantity")
                    .map_or(Ok(1), |v| whole(v, "quantity", 1))?,
            });
        }
        for q in tables(&t, "quests", &["quest", "phase"], &["quest", "phase"])? {
            let at = match &q["phase"] {
                toml::Value::String(w) if w.eq_ignore_ascii_case("done") => QuestAt::Done,
                v => QuestAt::Phase(whole(v, "phase", 1).map_err(|e| format!("{e}, or \"done\""))?),
            };
            r.quests.push(Quest {
                quest: whole(&q["quest"], "quest", 1)?,
                at,
            });
        }
        Ok(r)
    }

    /// The recipe with fields set by a TOML line (`level = 10`); a level set takes an experience
    /// away, and an experience a level.
    ///
    /// # Errors
    /// The line is not TOML, or the recipe it makes is not one.
    pub fn with_line(&self, line: &str) -> Result<Self, String> {
        let mut t: toml::Table = self
            .text()
            .parse()
            .map_err(|e| format!("{}: {e}", self.name))?;
        let set: toml::Table = line.parse().map_err(|e| format!("{line}: {e}"))?;
        for (k, v) in set {
            match k.as_str() {
                "level" => {
                    t.remove("experience");
                }
                "experience" => {
                    t.remove("level");
                }
                _ => {}
            }
            t.insert(k, v);
        }
        let text = toml::to_string(&t).map_err(|e| format!("{}: {e}", self.name))?;
        Self::parse(&self.name, &text)
    }

    /// Whether a character made as `made` is still this recipe's: the same body (race and sex)
    /// and the same build. Where it starts and whether it is a hero change nothing made.
    #[must_use]
    pub fn makes_same(&self, made: &Self) -> bool {
        self.race == made.race
            && self.sex == made.sex
            && self.level == made.level
            && self.experience == made.experience
            && self.money == made.money
            && self.skills == made.skills
            && self.items == made.items
            && self.quests == made.quests
    }
}

/// The recipes' folder of a world.
#[must_use]
pub fn folder(world: &Path) -> PathBuf {
    world.join(FOLDER)
}

/// A recipe's file.
#[must_use]
pub fn file(world: &Path, name: &str) -> PathBuf {
    folder(world).join(format!("{name}.toml"))
}

/// The world's recipes, by name, and a problem for each file that is not one.
#[must_use]
pub fn list(world: &Path) -> (Vec<Recipe>, Vec<String>) {
    let (mut out, mut problems) = (Vec::new(), Vec::new());
    let Ok(rd) = std::fs::read_dir(folder(world)) else {
        return (out, problems);
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "toml") {
            continue;
        }
        let Some(name) = p.file_stem().map(|s| s.to_string_lossy().to_string()) else {
            continue;
        };
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|t| Recipe::parse(&name, &t))
        {
            Ok(r) => out.push(r),
            Err(e) => problems.push(format!("{}: {e}", p.display())),
        }
    }
    out.sort_by_key(|r| r.name.to_lowercase());
    (out, problems)
}

/// Write a recipe into the world.
///
/// # Errors
/// A name the server would refuse, or a file that will not write.
pub fn save(world: &Path, r: &Recipe) -> Result<(), String> {
    check_name(&r.name)?;
    let f = file(world, &r.name);
    std::fs::create_dir_all(folder(world)).map_err(|e| e.to_string())?;
    std::fs::write(&f, r.text()).map_err(|e| format!("{}: {e}", f.display()))
}

/// Take a recipe away from the world, and its character from the playtest folder.
///
/// # Errors
/// A file that will not go.
pub fn delete(world: &Path, playtest: &Path, name: &str) -> Result<(), String> {
    let f = file(world, name);
    if f.exists() {
        std::fs::remove_file(&f).map_err(|e| format!("{}: {e}", f.display()))?;
    }
    reset(playtest, name)
}

/// A character's own folder in the playtest folder: how it was made, and the server's recipe it
/// was last given.
#[must_use]
pub fn made_dir(playtest: &Path, name: &str) -> PathBuf {
    playtest.join(FOLDER).join(name)
}

/// The server's recipe a character is made or given its build from.
#[must_use]
pub fn server_recipe(playtest: &Path, name: &str) -> PathBuf {
    made_dir(playtest, name).join("recipe.toml")
}

/// The recipe a character was last made or given its build from, when it has been made.
#[must_use]
pub fn made(playtest: &Path, name: &str) -> Option<Recipe> {
    let t = std::fs::read_to_string(made_dir(playtest, name).join("made.toml")).ok()?;
    Recipe::parse(name, &t).ok()
}

/// Write down that a character was made, or given its build, from `r`.
///
/// # Errors
/// A file that will not write.
pub fn mark_made(playtest: &Path, r: &Recipe) -> Result<(), String> {
    let d = made_dir(playtest, &r.name);
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    std::fs::write(d.join("made.toml"), r.text()).map_err(|e| e.to_string())
}

/// Write down that a character is not made (deleted, or to be made anew at its next play).
///
/// # Errors
/// A file that will not go.
pub fn unmark(playtest: &Path, name: &str) -> Result<(), String> {
    let f = made_dir(playtest, name).join("made.toml");
    match std::fs::remove_file(&f) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(format!("{}: {e}", f.display())),
        _ => Ok(()),
    }
}

/// Take a character's folder away from the playtest folder, so its next play makes it anew from
/// its recipe.
///
/// # Errors
/// A folder that will not go.
pub fn reset(playtest: &Path, name: &str) -> Result<(), String> {
    let d = made_dir(playtest, name);
    if d.exists() {
        std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    Ok(())
}

/// The store's file an earlier editor kept each character in, in its own folder.
const OWN_STORE: &str = "yalain.sqlite";

/// Take away what the playtest folder says was made where no store holds it: every character's
/// record when the store is not there (never made, or taken away), and each character an earlier
/// editor kept in a store of its own. Their next play makes them anew.
///
/// # Errors
/// A folder that will not go.
pub fn forget_unstored(playtest: &Path) -> Result<(), String> {
    let all = playtest.join(FOLDER);
    let Ok(rd) = std::fs::read_dir(&all) else {
        return Ok(());
    };
    let stored = playtest.join(STORE).is_file();
    for e in rd.flatten() {
        let d = e.path();
        if d.is_dir() && (!stored || d.join(OWN_STORE).exists()) {
            std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    Ok(())
}

/// What a play of a recipe asks of the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Making {
    /// Made as the recipe makes it (from the recipe it was made from): nothing to give, beyond a
    /// hero made or unmade.
    Same(Recipe),
    /// Made, and its build changed since (from the recipe it was made from): given what changed.
    Build(Recipe),
    /// Not made, or made of another race or sex: made anew.
    Make,
}

/// What a play of `r` asks of the playtest folder's store.
#[must_use]
pub fn making(playtest: &Path, r: &Recipe) -> Making {
    let stored =
        playtest.join(STORE).is_file() && !made_dir(playtest, &r.name).join(OWN_STORE).exists();
    match made(playtest, &r.name).filter(|_| stored) {
        Some(m) if m.race == r.race && m.sex == r.sex => {
            if r.makes_same(&m) {
                Making::Same(m)
            } else {
                Making::Build(m)
            }
        }
        _ => Making::Make,
    }
}

/// Where a character is exported to: the world's build folder, for another world to import.
#[must_use]
pub fn export_file(world: &Path, name: &str) -> PathBuf {
    world
        .join("build")
        .join(FOLDER)
        .join(format!("{name}.json"))
}

/// The qualities a character file's character holds its level and money in (the game's own ids).
const LEVEL: u32 = 302;
const MONEY: u32 = 3100;
/// A quest's status under way, and done (the game's own values).
const UNDERWAY: u32 = 0x1000_0000;
const COMPLETED: u32 = 0x3000_0000;
/// A held skill's mark of one trained.
const TRAINED: u32 = 0x1;

/// Bytes from hexadecimal text.
fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Little-endian numbers read in turn.
struct Le<'a>(&'a [u8]);

impl Le<'_> {
    fn u32(&mut self) -> Option<u32> {
        let (a, b) = self.0.split_first_chunk::<4>()?;
        self.0 = b;
        Some(u32::from_le_bytes(*a))
    }
    fn u64(&mut self) -> Option<u64> {
        let (a, b) = self.0.split_first_chunk::<8>()?;
        self.0 = b;
        Some(u64::from_le_bytes(*a))
    }
}

/// What a character holds in play, as the file keeps it (the server's encoding, versions 1 to
/// 10): its items, quests and skills, which lead the record in each.
fn held_in_play(bytes: &[u8]) -> Option<(Vec<Item>, Vec<Quest>, Vec<Skill>)> {
    let mut r = Le(bytes);
    let version = r.u32()?;
    if !(1..=10).contains(&version) {
        return None;
    }
    let (_money, _credits) = (r.u64()?, r.u32()?);
    let mut items: Vec<Item> = Vec::new();
    for _ in 0..r.u32()? {
        let (_id, template) = (r.u64()?, r.u32()?);
        let (_slot, _worn, quantity) = (r.u32()?, r.u32()?, r.u32()?);
        let quantity = quantity.max(1);
        match items.iter_mut().find(|i| i.template == template) {
            Some(i) => i.quantity = i.quantity.saturating_add(quantity),
            None => items.push(Item { template, quantity }),
        }
    }
    let mut quests = Vec::new();
    for _ in 0..r.u32()? {
        let (quest, status, phase) = (r.u32()?, r.u32()?, r.u32()?);
        let _times = (r.u64()?, r.u64()?);
        let at = match status {
            COMPLETED => QuestAt::Done,
            UNDERWAY if phase >= 1 => QuestAt::Phase(phase),
            _ => continue,
        };
        quests.push(Quest { quest, at });
    }
    let mut skills = Vec::new();
    for _ in 0..r.u32()? {
        let (skill, granted) = (r.u32()?, r.u32()?);
        let mask = if version == 1 {
            let _level = r.u32()?;
            TRAINED
        } else {
            let mask = r.u32()?;
            let _cost_and_kin = (r.u32()?, r.u32()?, r.u32()?);
            mask
        };
        let _spent = r.u64()?;
        // Trained by the player: a gift is no player's to train.
        if mask & TRAINED != 0 && granted == 0 {
            skills.push(Skill { skill, rating: 0 });
        }
    }
    Some((items, quests, skills))
}

/// A whole number of a character file's build, in the range `T` holds.
fn number<T: TryFrom<u64>>(v: &serde_json::Value, key: &str) -> Result<Option<T>, String> {
    match v.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(n) => n
            .as_u64()
            .and_then(|n| T::try_from(n).ok())
            .map(Some)
            .ok_or_else(|| format!("the build's {key}: {n} is not a whole number it can be")),
    }
}

/// The entries of a character file's build list `key`, each read by `entry`.
fn entries<T>(
    v: &serde_json::Value,
    key: &str,
    entry: impl Fn(&serde_json::Value) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    match v.get(key) {
        None | Some(serde_json::Value::Null) => Ok(Vec::new()),
        Some(serde_json::Value::Array(a)) => a.iter().map(entry).collect(),
        Some(_) => Err(format!("the build's {key} is not a list")),
    }
}

/// A character file's build (a version-2 file's `build`: what the character holds, in a recipe's
/// own words) as a recipe, field for field: its level or experience, money, whether it is a hero,
/// every skill it trained with its rating, its items by template and its quests done or under
/// way; with a place, it starts where it stood (`left`), else at the camera.
fn from_build(b: &serde_json::Value) -> Result<Recipe, String> {
    let name = b
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or("the build's name")?;
    let mut r = Recipe::new(name);
    r.race = number(b, "race")?.ok_or("the build's race")?;
    r.sex = number(b, "sex")?.ok_or("the build's sex")?;
    r.level = match b.get("level") {
        None | Some(serde_json::Value::Null) => None,
        Some(l) => Some(
            l.as_i64()
                .and_then(|l| i32::try_from(l).ok())
                .ok_or_else(|| format!("the build's level: {l}"))?,
        ),
    };
    r.experience = number(b, "experience")?;
    r.money = number(b, "money")?;
    r.hero = match b.get("hero") {
        None | Some(serde_json::Value::Null) => false,
        Some(h) => h.as_bool().ok_or("the build's hero is not true or false")?,
    };
    if b.get("place").is_some_and(|p| !p.is_null()) {
        r.start = Start::Left;
    }
    r.skills = entries(b, "skills", |s| {
        Ok(Skill {
            skill: number(s, "skill")?.ok_or("a skill without its skill")?,
            rating: number(s, "rating")?.ok_or("a skill without its rating")?,
        })
    })?;
    r.items = entries(b, "items", |i| {
        Ok(Item {
            template: number(i, "template")?.ok_or("an item without its template")?,
            quantity: number(i, "quantity")?.unwrap_or(1),
        })
    })?;
    r.quests = entries(b, "quests", |q| {
        let quest: u32 = number(q, "quest")?.ok_or("a quest without its quest")?;
        let at = match q.get("phase") {
            Some(serde_json::Value::String(w)) if w.eq_ignore_ascii_case("done") => QuestAt::Done,
            Some(p) => match p.as_u64().and_then(|n| u32::try_from(n).ok()) {
                Some(n) if n >= 1 => QuestAt::Phase(n),
                _ => {
                    return Err(format!(
                        "quest {quest:08X}: the phase {p} is neither done nor 1 or more"
                    ))
                }
            },
            None => return Err(format!("quest {quest:08X}: no phase")),
        };
        Ok(Quest { quest, at })
    })?;
    Ok(r)
}

/// A character file's character (the server's export, another world's) as a recipe: its build,
/// as the file gives it in a recipe's own words ([`from_build`]): its level, money and hero, the
/// skills it trained with their ratings, the items it holds (worn ones too), its quests done or
/// under way, and where it stood. Written as its recipe, a build the server gives it changes none
/// of that; a build made from it anew holds it all.
///
/// A file written before files carried a build (version 1) is read from the server's own
/// encodings: its level and money from its qualities, and the skills it trained (a rating of 0:
/// trained, as high as it has raised them), its items and its quests from the leading fields of
/// its play state; a state that does not read gives its body alone.
///
/// # Errors
/// It does not read, is not a character file, or its build does not read.
pub fn exported(file: &Path) -> Result<Recipe, String> {
    let at = |e: &dyn std::fmt::Display| format!("{}: {e}", file.display());
    let text = std::fs::read_to_string(file).map_err(|e| at(&e))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| at(&e))?;
    if v.get("format").and_then(serde_json::Value::as_str) != Some("yalain-character") {
        return Err(at(&"not a character file"));
    }
    if let Some(b) = v.get("build").filter(|b| !b.is_null()) {
        return from_build(b).map_err(|e| at(&e));
    }
    let c = v.get("character").ok_or_else(|| at(&"no character"))?;
    let num = |k: &str| {
        c.get(k)
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| at(&format!("the character's {k}")))
    };
    let name = c
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| at(&"the character's name"))?;
    let mut r = Recipe::new(name);
    r.race = num("race")?;
    r.sex = num("sex")?;
    let state = |k: &str| {
        v.get("state")
            .and_then(|s| s.get(k))
            .and_then(serde_json::Value::as_str)
            .and_then(unhex)
    };
    if let Some(q) = state("qualities").and_then(|b| Qualities::read(&mut Reader::new(&b)).ok()) {
        r.level = q.ints.get(LEVEL).copied().filter(|l| *l > 1);
        r.money = q
            .ints
            .get(MONEY)
            .and_then(|m| u32::try_from(*m).ok())
            .filter(|m| *m > 0);
    }
    if let Some((items, quests, skills)) = state("play").and_then(|b| held_in_play(&b)) {
        (r.items, r.quests, r.skills) = (items, quests, skills);
    }
    Ok(r)
}
