//! Playtest characters: recipes kept with the world, each a player to play the world as, made by
//! the editor's own Yalain.
//!
//! A recipe is `characters/<Name>.toml` beside `world.toml`. It says what the character is and
//! where it starts:
//!
//! ```toml
//! race = "human"      # human, lugian, tumerok, drudge or empyrean
//! sex = "female"      # male or female
//! start = "camera"    # camera, selection, left, or "place <key>"
//! hero = false        # a hero earns focus, its third vital
//! ```
//!
//! `start` is where Play puts it: on the ground under the camera, facing as the camera faces; in
//! front of the selected row, facing it; where it stood when it last left the world; or at one of
//! the world's places. The world database reads only its packs' folders, so the recipes are never
//! part of the world the server serves.
//!
//! The character itself lives in a store of its own in the playtest folder
//! (`.playtest/characters/<Name>/`): the server makes it there from its recipe the first time it
//! plays, as it makes any character it is told to, and keeps what it finds, learns and earns
//! there after. Beside the store, `made.toml` is the recipe as it was made. A recipe whose race or
//! sex changed since is made again; Reset takes the store away, so the next play makes it anew.

use std::path::{Path, PathBuf};

use auberean_world::chargen;

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

/// A playtest character's recipe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    pub name: String,
    /// The race and sex, as the server takes them.
    pub race: u32,
    pub sex: u32,
    pub start: Start,
    pub hero: bool,
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

impl Recipe {
    /// A new recipe: a human man who starts at the camera.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            race: chargen::HUMAN,
            sex: chargen::MALE,
            start: Start::Camera,
            hero: false,
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

    /// The recipe's text.
    #[must_use]
    pub fn text(&self) -> String {
        format!(
            "# A playtest character: what Athenaeum's Play makes on its own Yalain.\n\
             race = \"{}\"\nsex = \"{}\"\nstart = \"{}\"\nhero = {}\n",
            self.race_word(),
            self.sex_word(),
            self.start.word(),
            self.hero
        )
    }

    /// The recipe of `name` from its text.
    ///
    /// # Errors
    /// Text that is not TOML, or a field that is not one.
    pub fn parse(name: &str, text: &str) -> Result<Self, String> {
        check_name(name)?;
        let t: toml::Table = text.parse().map_err(|e| format!("{name}: not TOML: {e}"))?;
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
        for k in t.keys() {
            if !["race", "sex", "start", "hero"].contains(&k.as_str()) {
                return Err(format!("{name}: {k} is not a recipe's field"));
            }
        }
        Ok(r)
    }

    /// Whether a character made as `made` is still this recipe's: the same body (race and sex).
    #[must_use]
    pub fn same_body(&self, made: &Self) -> bool {
        self.race == made.race && self.sex == made.sex
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

/// A character's own folder in the playtest folder: its store and how it was made.
#[must_use]
pub fn made_dir(playtest: &Path, name: &str) -> PathBuf {
    playtest.join(FOLDER).join(name)
}

/// The store a character lives in.
#[must_use]
pub fn store(playtest: &Path, name: &str) -> PathBuf {
    made_dir(playtest, name).join("yalain.sqlite")
}

/// The recipe a character was made from, when it has been made.
#[must_use]
pub fn made(playtest: &Path, name: &str) -> Option<Recipe> {
    let t = std::fs::read_to_string(made_dir(playtest, name).join("made.toml")).ok()?;
    Recipe::parse(name, &t).ok()
}

/// Write down that a character was made from `r`.
///
/// # Errors
/// A file that will not write.
pub fn mark_made(playtest: &Path, r: &Recipe) -> Result<(), String> {
    let d = made_dir(playtest, &r.name);
    std::fs::create_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    std::fs::write(d.join("made.toml"), r.text()).map_err(|e| e.to_string())
}

/// Take a character away from the playtest folder, so its next play makes it from its recipe.
///
/// # Errors
/// A folder that will not go (its store open in a running server, say).
pub fn reset(playtest: &Path, name: &str) -> Result<(), String> {
    let d = made_dir(playtest, name);
    if d.exists() {
        std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    Ok(())
}

/// What a play of `r` asks of its store: whether the character is to be made (anew), and whether
/// the store there is to be taken away first (its body changed).
#[must_use]
pub fn to_make(playtest: &Path, r: &Recipe) -> (bool, bool) {
    let stored = store(playtest, &r.name).exists();
    match made(playtest, &r.name) {
        Some(m) if stored && r.same_body(&m) => (false, false),
        Some(_) => (true, true),
        None => (true, stored),
    }
}
