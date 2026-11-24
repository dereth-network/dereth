//! A world as files: one per stable key, under a folder per kind, and back.
//!
//! A file's name is the key it holds, made safe for a file system; a reader takes each row's key
//! from the row, never from the name.

use std::collections::BTreeMap;
use std::path::Path;

use empyrean_common::dotnet::DotNetDateTime;
use empyrean_common::era::EraId;
use empyrean_content::models::world::{TreasureMaterialGroups, TreasureWielded};
use empyrean_content::WorldContent;
use toml::Table;

use crate::kinds;
use crate::row::{file, Row};
use crate::value::{id_of, time_of, time_text, R, STAMP};

/// The format the files are in.
pub const FORMAT: u32 = 1;

/// The file naming the world's era, its format and the time a row carries when it says none.
pub const META: &str = "world-db.toml";

/// A name as a file name: letters, digits, `-`, `_` and `.` kept, anything else `_`.
#[must_use]
pub fn file_safe(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s.starts_with('.') {
        format!("_{s}")
    } else {
        s
    }
}

/// The world's stamp: ACE's, unless the world names another.
#[must_use]
pub fn default_stamp() -> DotNetDateTime {
    time_of(&toml::Value::String(STAMP.into())).unwrap_or_default()
}

/// Files being written: each a list of rows of one kind.
#[derive(Debug, Default)]
struct Out {
    files: BTreeMap<String, (&'static str, Vec<Row>)>,
    /// Each named file's path, by the path case folded: names that differ only in case (or in
    /// what a file name can't hold) share the first one's file, so a file system that ignores
    /// case reads what was written.
    taken: BTreeMap<String, String>,
}

impl Out {
    fn add(&mut self, path: String, kind: &'static str, row: Row) {
        self.files
            .entry(path)
            .or_insert((kind, Vec::new()))
            .1
            .push(row);
    }

    /// The file for the name `name` in `dir`.
    fn named(&mut self, dir: &str, name: &str) -> String {
        let path = format!("{dir}/{}.toml", file_safe(name));
        self.taken
            .entry(path.to_ascii_lowercase())
            .or_insert(path)
            .clone()
    }
}

/// `content` as files, by path (`weenie/00509.toml`), each row with `stamp` as its default time.
pub fn write(content: &WorldContent, stamp: DotNetDateTime) -> R<BTreeMap<String, String>> {
    let mut o = Out::default();
    for part in PARTS {
        add_part(content, part, stamp, &mut o)?;
    }
    let mut out: BTreeMap<String, String> = o
        .files
        .into_iter()
        .map(|(path, (kind, rows))| (path, file(kind, &rows)))
        .collect();
    let stamp_text = time_text(stamp).text();
    out.insert(
        META.into(),
        format!(
            "# The world's rows: their format, the era they're for, and the time a row carries when it names none.\nformat = {FORMAT}\nera = \"{}\"\nstamp = {stamp_text}\n",
            content.era.name()
        ),
    );
    Ok(out)
}

/// One kind of row a world is written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Weenie,
    Placement,
    Encounter,
    CookBook,
    Recipe,
    RecipeVital,
    Spell,
    Quest,
    Event,
    Poi,
    HousePortal,
    TreasureDeath,
    GroupedTreasure,
    Wielded,
    GemCount,
    MaterialBase,
    MaterialColor,
    MaterialGroups,
    LootPolicy,
    Version,
}

/// Every kind of row, in the order their files' paths sort.
const PARTS: [Part; 20] = [
    Part::CookBook,
    Part::Encounter,
    Part::Event,
    Part::GroupedTreasure,
    Part::HousePortal,
    Part::Placement,
    Part::LootPolicy,
    Part::Poi,
    Part::Quest,
    Part::Recipe,
    Part::RecipeVital,
    Part::Spell,
    Part::TreasureDeath,
    Part::GemCount,
    Part::MaterialBase,
    Part::MaterialColor,
    Part::MaterialGroups,
    Part::Wielded,
    Part::Version,
    Part::Weenie,
];

/// The rows of `content`'s `part`, added to `o`.
fn add_part(content: &WorldContent, part: Part, stamp: DotNetDateTime, o: &mut Out) -> R<()> {
    match part {
        Part::Weenie => {
            for w in &content.weenies {
                o.add(
                    format!("weenie/{:05}.toml", w.class_id),
                    "weenie",
                    kinds::weenie_row(w, stamp),
                );
            }
        }
        Part::Placement => {
            for i in &content.landblock_instances {
                o.add(
                    format!("landblock/{:04X}.toml", i.obj_cell_id >> 16),
                    "placement",
                    kinds::placement_row(i, stamp),
                );
            }
        }
        Part::Encounter => {
            for e in &content.encounters {
                #[allow(clippy::cast_sign_loss)]
                let block = e.landblock as u32;
                o.add(
                    format!("encounter/{:02X}xx.toml", block >> 8),
                    "encounter",
                    kinds::encounter_row(e, stamp),
                );
            }
        }
        Part::CookBook => {
            for c in &content.cook_books {
                o.add(
                    format!("cook_book/{:05}-{:05}.toml", c.source_wcid, c.target_wcid),
                    "cook_book",
                    kinds::cook_book_row(c, stamp),
                );
            }
        }
        Part::Recipe => {
            for r in &content.recipes {
                o.add(
                    format!("recipe/{}.toml", r.id),
                    "recipe",
                    kinds::recipe_row(r, stamp),
                );
            }
        }
        Part::RecipeVital => {
            for v in &content.recipe_vital_requirements {
                o.add(
                    format!("recipe_vital/{}.toml", v.id),
                    "recipe_vital",
                    kinds::keyed_row("recipe", v.id, v, stamp),
                );
            }
        }
        Part::Spell => {
            for s in &content.spells {
                o.add(
                    format!("spell/{}.toml", s.id),
                    "spell",
                    kinds::keyed_row("id", s.id, s, stamp),
                );
            }
        }
        Part::Quest => {
            for q in &content.quests {
                let path = o.named("quest", &q.name);
                o.add(path, "quest", kinds::plain_row(q, stamp));
            }
        }
        Part::Event => {
            for e in &content.events {
                let path = o.named("event", &e.name);
                o.add(path, "event", kinds::plain_row(e, stamp));
            }
        }
        Part::Poi => {
            for p in &content.points_of_interest {
                let path = o.named("poi", &p.name);
                o.add(path, "poi", kinds::plain_row(p, stamp));
            }
        }
        Part::HousePortal => {
            for h in &content.house_portals {
                o.add(
                    format!("house_portal/{}.toml", h.house_id),
                    "house_portal",
                    kinds::plain_row(h, stamp),
                );
            }
        }
        Part::TreasureDeath => {
            for t in &content.treasure_death {
                o.add(
                    format!("treasure_death/{}.toml", t.treasure_type),
                    "treasure_death",
                    kinds::keyed_row("type", t.treasure_type, t, stamp),
                );
            }
        }
        Part::GroupedTreasure => {
            for g in &content.grouped_treasure {
                o.add(
                    format!("grouped_treasure/{}.toml", g.treasure_type),
                    "grouped_treasure",
                    kinds::grouped_treasure_row(g)?,
                );
            }
        }
        Part::Wielded => {
            for (ty, rows) in by_type(&content.treasure_wielded) {
                o.add(
                    format!("treasure_wielded/{ty:05}.toml"),
                    "treasure_wielded",
                    kinds::wielded_row(ty, &rows, stamp),
                );
            }
        }
        Part::GemCount => {
            for t in &content.treasure_gem_count {
                o.add(
                    "treasure_gem_count.toml".into(),
                    "treasure_gem_count",
                    kinds::plain_row(t, stamp),
                );
            }
        }
        Part::MaterialBase => {
            for t in &content.treasure_material_base {
                o.add(
                    "treasure_material_base.toml".into(),
                    "treasure_material_base",
                    kinds::plain_row(t, stamp),
                );
            }
        }
        Part::MaterialColor => {
            for t in &content.treasure_material_color {
                o.add(
                    "treasure_material_color.toml".into(),
                    "treasure_material_color",
                    kinds::plain_row(t, stamp),
                );
            }
        }
        Part::MaterialGroups => {
            // A group's materials at a tier are one list: a world may list one material twice.
            let mut groups: BTreeMap<(u32, u32), Vec<TreasureMaterialGroups>> = BTreeMap::new();
            for t in &content.treasure_material_groups {
                groups
                    .entry((t.material_group, t.tier))
                    .or_default()
                    .push(t.clone());
            }
            for ((group, tier), rows) in groups {
                o.add(
                    "treasure_material_groups.toml".into(),
                    "treasure_material_groups",
                    kinds::material_group_row(group, tier, &rows, stamp),
                );
            }
        }
        Part::LootPolicy => {
            for p in &content.loot_policies {
                o.add(
                    "loot_policy.toml".into(),
                    "loot_policy",
                    kinds::loot_policy_row(p)?,
                );
            }
        }
        Part::Version => {
            for v in &content.versions {
                o.add("version.toml".into(), "version", kinds::plain_row(v, stamp));
            }
        }
    }
    Ok(())
}

/// `content` without the tables of `part`.
fn release(content: &mut WorldContent, part: Part) {
    match part {
        Part::Weenie => content.weenies = Vec::new(),
        Part::Placement => content.landblock_instances = Vec::new(),
        Part::Encounter => content.encounters = Vec::new(),
        Part::CookBook => content.cook_books = Vec::new(),
        Part::Recipe => content.recipes = Vec::new(),
        Part::RecipeVital => content.recipe_vital_requirements = Vec::new(),
        Part::Spell => content.spells = Vec::new(),
        Part::Quest => content.quests = Vec::new(),
        Part::Event => content.events = Vec::new(),
        Part::Poi => content.points_of_interest = Vec::new(),
        Part::HousePortal => content.house_portals = Vec::new(),
        Part::TreasureDeath => content.treasure_death = Vec::new(),
        Part::GroupedTreasure => content.grouped_treasure = Vec::new(),
        Part::Wielded => content.treasure_wielded = Vec::new(),
        Part::GemCount => content.treasure_gem_count = Vec::new(),
        Part::MaterialBase => content.treasure_material_base = Vec::new(),
        Part::MaterialColor => content.treasure_material_color = Vec::new(),
        Part::MaterialGroups => content.treasure_material_groups = Vec::new(),
        Part::LootPolicy => content.loot_policies = Vec::new(),
        Part::Version => content.versions = Vec::new(),
    }
}

/// Each of `content`'s rows, with its kind and the path of the file [`write()`] puts it in, in the
/// order of their paths and, within a file, the order [`write()`] writes them; each table of
/// `content` dropped once its rows are made, so a world and all its rows are never held at once.
///
/// # Errors
/// What `each` returns, and a row that can't be written.
pub fn each_row(
    mut content: WorldContent,
    stamp: DotNetDateTime,
    mut each: impl FnMut(&str, &'static str, &Row) -> R<()>,
) -> R<()> {
    for part in PARTS {
        let mut o = Out::default();
        add_part(&content, part, stamp, &mut o)?;
        release(&mut content, part);
        // Each row let go once given.
        for (path, (kind, rows)) in o.files {
            for r in rows {
                each(&path, kind, &r)?;
            }
        }
    }
    Ok(())
}

/// A wielded type's rows, in their order, by type.
fn by_type(rows: &[TreasureWielded]) -> BTreeMap<u32, Vec<TreasureWielded>> {
    let mut out: BTreeMap<u32, Vec<TreasureWielded>> = BTreeMap::new();
    for r in rows {
        out.entry(r.treasure_type).or_default().push(r.clone());
    }
    out
}

/// A pack's header, beside its rows.
pub const PACK: &str = "pack.toml";

/// What a folder of rows says of itself (its [`META`] file).
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub era: EraId,
    /// The time a row carries when it names none.
    pub stamp: DotNetDateTime,
}

/// `files` as the pack `base`, its rows said to come from `source` at `revision` (`from =
/// "source"`): its header (`pack.toml`), written.
///
/// # Errors
/// Never; the result is for the callers' `?`.
pub fn set_source(files: &mut BTreeMap<String, String>, source: &str, revision: &str) -> R<()> {
    let q = |s: &str| crate::value::V::Str(s.into()).text();
    files.insert(
        PACK.into(),
        format!(
            "# The base: a whole known world, converted from another project's.\nformat_version = 1\nname = \"base\"\nversion = \"0.1.0\"\ngame = \"ac1\"\n# Where a row came from when it says nothing.\nprov = {{ from = \"source\", source = {}, revision = {} }}\n",
            q(source),
            q(revision)
        ),
    );
    Ok(())
}

/// What `files` say of themselves, from their meta file.
///
/// # Errors
/// No meta file, or one that doesn't read or names a format or era this reader doesn't know.
pub fn meta(files: &BTreeMap<String, String>) -> R<Meta> {
    let text = files.get(META).ok_or_else(|| format!("no {META}"))?;
    let t: Table = toml::from_str(text).map_err(|e| format!("{META}: {e}"))?;
    let format = t.get("format").map(id_of).transpose()?.unwrap_or(0);
    if format != FORMAT {
        return Err(format!(
            "{META}: format {format}, and this reader knows {FORMAT}"
        ));
    }
    let era = t
        .get("era")
        .and_then(toml::Value::as_str)
        .and_then(EraId::parse)
        .ok_or_else(|| format!("{META}: no era it knows"))?;
    let stamp = t
        .get("stamp")
        .map(time_of)
        .transpose()?
        .unwrap_or_else(default_stamp);
    Ok(Meta { era, stamp })
}

/// `files` without their meta file and pack header: their rows alone.
#[must_use]
pub fn rows_only(files: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    files
        .iter()
        .filter(|(p, _)| p.as_str() != META && p.as_str() != PACK)
        .map(|(p, t)| (p.clone(), t.clone()))
        .collect()
}

/// The world `files` hold, read as one layer: its rows a whole known world, each item once. Its
/// ids are not yet numbered ([`crate::canon::number`] does that).
pub fn read(files: &BTreeMap<String, String>) -> R<WorldContent> {
    let m = meta(files)?;
    let known = [
        "weenie",
        "landblock",
        "encounter",
        "cook_book",
        "recipe",
        "recipe_vital",
        "spell",
        "quest",
        "event",
        "poi",
        "house_portal",
        "treasure_death",
        "grouped_treasure",
        "treasure_wielded",
        "treasure_gem_count",
        "treasure_material_base",
        "treasure_material_color",
        "treasure_material_groups",
        "loot_policy",
        "version",
    ];
    for path in files.keys() {
        let top = path
            .split('/')
            .next()
            .unwrap_or("")
            .trim_end_matches(".toml");
        if path != META && path != PACK && !known.contains(&top) {
            return Err(format!("{path}: no kind of row lives here"));
        }
    }
    crate::layers::fold_layers(
        crate::layers::Layers {
            base: files.clone(),
            ..Default::default()
        },
        m.era,
        m.stamp,
    )
}

/// `files` written under `dir`, which must be empty or absent.
pub fn write_dir(dir: &Path, files: &BTreeMap<String, String>) -> R<()> {
    if dir.exists()
        && std::fs::read_dir(dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .next()
            .is_some()
    {
        return Err(format!(
            "{} isn't empty: rows are written whole",
            dir.display()
        ));
    }
    for (path, text) in files {
        let full = dir.join(path);
        if let Some(parent) = full.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&full, text).map_err(|e| format!("{}: {e}", full.display()))?;
    }
    Ok(())
}

/// Every `.toml` file under `dir`, by its path with `/` between parts.
pub fn read_dir(dir: &Path) -> R<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(&d).map_err(|e| format!("{}: {e}", d.display()))? {
            let p = e.map_err(|e| e.to_string())?.path();
            if p.is_dir() {
                todo.push(p);
            } else if p.extension().is_some_and(|x| x == "toml") {
                let rel = p
                    .strip_prefix(dir)
                    .map_err(|e| e.to_string())?
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(
                    rel,
                    std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?,
                );
            }
        }
    }
    Ok(out)
}
