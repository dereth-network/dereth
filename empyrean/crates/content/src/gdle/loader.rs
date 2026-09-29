// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Adapter/GDLE/GDLELoader.cs
//! `GDLELoader`: GDLE JSON files (landblock spawn maps, world spawns, events, quests, spells,
//! recipes and precursors, regions, terrain, wielded treasure) read into the GDLE models, and the
//! `...Converted` variants that turn them into World rows through `GDLEConverter`.
//!
//! Each C# method is `bool TryLoad...(..., out result)` around a `try`/`catch` that turns any
//! exception into `false` with a null result. Here that is `R<...>`: `Err` for `false` (with the
//! reason), `Ok` for `true`. A document that is JSON `null` loads (`true`) as a null model, which is
//! `Ok(None)`.
//!
//! ACE's `...InParallel` methods run `Parallel.ForEach` into a `ConcurrentBag`, whose enumeration
//! order is unspecified. Here every folder is read in [`crate::import::patch::expand`]'s order (the
//! relative paths, compared component by component).

use std::path::{Path, PathBuf};

use empyrean_common::dotnet::DotNetDict;

use super::converter;
use super::models::{Event, Region, Spells, TerrainData, WieldedTreasureTable, WorldSpawns};
use crate::import::json::gdle;
use crate::import::json::models::{Landblock, Quest, Recipe, RecipeCombined, RecipePrecursor};
use crate::import::json::value::{self, list, Json, R};
use crate::import::json::world;
use crate::import::patch::{expand, Input, InputKind};
use crate::models::world::{
    Event as WorldEvent, LandblockInstance, LandblockInstanceLink, Spell, TreasureWielded,
};

/// `File.ReadAllText(file)`: the encoding from the byte order mark (UTF-8, UTF-16 LE/BE, UTF-32
/// LE), else UTF-8; invalid sequences become U+FFFD; the mark itself is dropped.
///
/// Not ACE code: .NET's `StreamReader` detection, as ACE's loaders rely on it.
pub fn read_all_text(file: &Path) -> R<String> {
    let bytes = std::fs::read(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let units16 = |b: &[u8], be: bool| -> Vec<u16> {
        b.as_chunks::<2>()
            .0
            .iter()
            .map(|&c| {
                if be {
                    u16::from_be_bytes(c)
                } else {
                    u16::from_le_bytes(c)
                }
            })
            .collect()
    };
    Ok(match bytes.as_slice() {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, 0, 0, rest @ ..] => rest
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&c| char::from_u32(u32::from_le_bytes(c)).unwrap_or('\u{FFFD}'))
            .collect(),
        [0xFF, 0xFE, rest @ ..] => String::from_utf16_lossy(&units16(rest, false)),
        [0xFE, 0xFF, rest @ ..] => String::from_utf16_lossy(&units16(rest, true)),
        rest => String::from_utf8_lossy(rest).into_owned(),
    })
}

/// `JsonSerializer.Deserialize<T>(text)`.
fn deserialize<T>(text: &str, read: impl FnOnce(&Json) -> R<Option<T>>) -> R<Option<T>> {
    let doc = value::parse(text)?;
    read(&doc)
}

/// `JsonSerializer.Deserialize<T>(File.ReadAllText(file))`.
fn load<T>(file: &Path, read: impl FnOnce(&Json) -> R<Option<T>>) -> R<Option<T>> {
    deserialize(&read_all_text(file)?, read)
}

/// `JsonSerializer.Deserialize<List<T>>(text)` of a list of class values.
fn deserialize_list<T>(
    text: &str,
    what: &str,
    read: fn(&Json, &str) -> R<Option<T>>,
) -> R<Option<Vec<Option<T>>>> {
    deserialize(text, |doc| list(doc, what, read))
}

/// `JsonSerializer.Deserialize<List<T>>(File.ReadAllText(file))`.
fn load_list<T>(
    file: &Path,
    what: &str,
    read: fn(&Json, &str) -> R<Option<T>>,
) -> R<Option<Vec<Option<T>>>> {
    deserialize_list(&read_all_text(file)?, what, read)
}

/// `Directory.GetFiles(folder, "*.json", SearchOption.AllDirectories)`, in path order.
fn json_files(folder: &Path) -> R<Vec<PathBuf>> {
    let meta = std::fs::metadata(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    if !meta.is_dir() {
        // GetFiles on a file throws IOException ("The directory name is invalid").
        return Err(format!("{} is not a directory", folder.display()));
    }
    expand(&Input {
        kind: InputKind::Json,
        path: folder.to_path_buf(),
    })
    .map_err(|e| e.to_string())
}

fn not_null<'a, T>(v: &'a Option<T>, what: &str) -> R<&'a T> {
    v.as_ref().ok_or_else(|| format!("{what} is null"))
}

// ACE: GDLELoader.TryLoadLandblock
pub fn try_load_landblock(file: &Path) -> R<Option<Landblock>> {
    load(file, Landblock::read)
}

/// Every landblock file under `folder` that loads (a `null` document included, as `None`).
// ACE: GDLELoader.TryLoadLandblocksInParallel
pub fn try_load_landblocks_in_parallel(folder: &Path) -> R<Vec<Option<Landblock>>> {
    let files = json_files(folder)?;

    // DIVERGE: Parallel.ForEach into a ConcurrentBag (unspecified order); files in path order here.
    let mut landblocks = Vec::new();
    for file in files {
        if let Ok(result) = try_load_landblock(&file) {
            landblocks.push(result);
        }
    }

    Ok(landblocks)
}

// ACE: GDLELoader.TryLoadWorldSpawns
pub fn try_load_world_spawns(file: &Path) -> R<Option<WorldSpawns>> {
    load(file, |doc| WorldSpawns::read(doc, "WorldSpawns"))
}

/// Landblock instances and links, with ACE's guids (see [`re_guid_and_convert_landblocks`]).
pub type ConvertedLandblocks = (Vec<LandblockInstance>, Vec<LandblockInstanceLink>);

/// This will sanitize the Guids for ACE use to the following format: 0x7LBID### where ### starts
/// from `starting_id_offset`.
// ACE: GDLELoader.TryLoadLandblocksConverted
pub fn try_load_landblocks_converted(
    folder: &Path,
    starting_id_offset: u16,
) -> R<ConvertedLandblocks> {
    // TryLoadLandblocksInParallel returns false with a null list; the null is what is tested.
    let mut landblocks = try_load_landblocks_in_parallel(folder)?;

    re_guid_and_convert_landblocks(starting_id_offset, &mut landblocks)
}

// ACE: GDLELoader.ReGuidAndConvertLandblocks
fn re_guid_and_convert_landblocks(
    starting_id_offset: u16,
    landblocks: &mut [Option<Landblock>],
) -> R<ConvertedLandblocks> {
    let mut id_changes: DotNetDict<u32 /*LBID*/, DotNetDict<u32 /*from*/, u32 /*to*/>> =
        DotNetDict::new();

    // First we convert all weenies
    for landblock in landblocks.iter_mut() {
        let landblock = landblock.as_mut().ok_or("landblock is null")?;
        let mut current_offset = starting_id_offset;

        if !id_changes.contains_key(&landblock.key) {
            id_changes.add(landblock.key, DotNetDict::new());
        }

        let key = landblock.key;
        let value = landblock.value.as_mut().ok_or("landblock value is null")?;
        for weenie in value.weenies.as_mut().ok_or("weenies is null")? {
            let weenie = weenie.as_mut().ok_or("weenie is null")?;
            let pos = not_null(&weenie.pos, "weenie pos")?;
            let new_guid =
                0x7000_0000 | ((pos.land_cell_id & 0xFFFF_0000) >> 4) | u32::from(current_offset);
            // ushort++ wraps.
            current_offset = current_offset.wrapping_add(1);

            let changes = id_changes
                .get_mut(&key)
                .ok_or("idChanges[key] is missing")?;
            if !changes.contains_key(&weenie.id) {
                changes.add(weenie.id, new_guid);
                weenie.id = new_guid;
            }
        }
    }

    // Then we update all the links
    for landblock in landblocks.iter_mut() {
        let landblock = landblock.as_mut().ok_or("landblock is null")?;
        let key = landblock.key;
        let value = landblock.value.as_mut().ok_or("landblock value is null")?;
        let Some(links) = value.links.as_mut() else {
            continue;
        };

        let changes = id_changes.get(&key).ok_or("idChanges[key] is missing")?;
        for link in links {
            let link = link.as_mut().ok_or("link is null")?;
            link.source = changes.get(&link.source).copied().unwrap_or(0);
            link.target = changes.get(&link.target).copied().unwrap_or(0);
        }
    }

    Ok(try_convert_landblocks(landblocks))
}

// ACE: GDLELoader.TryConvertLandblocks
fn try_convert_landblocks(landblocks: &[Option<Landblock>]) -> ConvertedLandblocks {
    let mut results: Vec<LandblockInstance> = Vec::new();
    let mut links: Vec<LandblockInstanceLink> = Vec::new();

    for value in landblocks {
        // GDLEConverter.TryConvert catches a null landblock (NullReferenceException) as false.
        let Some(value) = value else { continue };
        if let Ok((part1, part2)) = gdle::try_convert_landblock(value) {
            let part2: Vec<LandblockInstanceLink> = part2.iter().map(world_link).collect();
            for instance in &part1 {
                let mut instance = world_instance(instance);
                for link in &part2 {
                    if link.parent_guid == instance.guid {
                        instance.landblock_instance_link.push(link.clone());
                    }
                }

                results.push(instance);
            }

            for part in part2 {
                if let Some(child) = results.iter_mut().find(|x| x.guid == part.child_guid) {
                    child.is_link_child = true;
                }
                links.push(part);
            }
        }
    }

    (results, links)
}

/// The World model of a converted instance (C# converts straight into it).
fn world_instance(i: &world::LandblockInstance) -> LandblockInstance {
    LandblockInstance {
        guid: i.guid,
        landblock: None,
        weenie_class_id: i.weenie_class_id,
        obj_cell_id: i.obj_cell_id,
        origin_x: i.origin_x,
        origin_y: i.origin_y,
        origin_z: i.origin_z,
        angles_w: i.angles_w,
        angles_x: i.angles_x,
        angles_y: i.angles_y,
        angles_z: i.angles_z,
        is_link_child: i.is_link_child,
        last_modified: i.last_modified,
        landblock_instance_link: Vec::new(),
    }
}

fn world_link(l: &world::LandblockInstanceLink) -> LandblockInstanceLink {
    LandblockInstanceLink {
        id: 0,
        parent_guid: l.parent_guid,
        child_guid: l.child_guid,
        last_modified: l.last_modified,
    }
}

/// This will sanitize the Guids for ACE use to the following format: 0x7LBID### where ### starts
/// from `starting_id_offset`.
// ACE: GDLELoader.TryLoadWorldSpawnsConverted
pub fn try_load_world_spawns_converted(
    file: &Path,
    starting_id_offset: u16,
) -> R<ConvertedLandblocks> {
    world_spawns_converted(&read_all_text(file)?, starting_id_offset)
}

/// [`try_load_world_spawns_converted`] over the file's text.
pub(crate) fn world_spawns_converted(
    text: &str,
    starting_id_offset: u16,
) -> R<ConvertedLandblocks> {
    let gdle_model = deserialize(text, |doc| WorldSpawns::read(doc, "WorldSpawns"))?;
    let gdle_model = gdle_model.ok_or("the document is null")?;

    let Some(mut landblocks) = gdle_model.landblocks else {
        return Err("landblocks is null".to_owned());
    };

    re_guid_and_convert_landblocks(starting_id_offset, &mut landblocks)
}

// ACE: GDLELoader.TryLoadEvents
pub fn try_load_events(file: &Path) -> R<Option<Vec<Option<Event>>>> {
    load_list(file, "List<Event>", Event::read)
}

// ACE: GDLELoader.TryLoadEventsConverted
pub fn try_load_events_converted(file: &Path) -> R<Vec<WorldEvent>> {
    events_converted(&read_all_text(file)?)
}

/// [`try_load_events_converted`] over the file's text.
pub(crate) fn events_converted(text: &str) -> R<Vec<WorldEvent>> {
    let gdle_model =
        deserialize_list(text, "List<Event>", Event::read)?.ok_or("the document is null")?;

    let mut results = Vec::new();

    for value in gdle_model.iter().flatten() {
        if let Ok(result) = converter::try_convert_event(value) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: GDLELoader.TryLoadQuest
pub fn try_load_quest(file: &Path) -> R<Option<Quest>> {
    load(file, Quest::read)
}

// ACE: GDLELoader.TryLoadQuests
pub fn try_load_quests(file: &Path) -> R<Option<Vec<Option<Quest>>>> {
    load_list(file, "List<Quest>", |e, _| Quest::read(e))
}

// ACE: GDLELoader.TryLoadQuestsConverted
pub fn try_load_quests_converted(file: &Path) -> R<Vec<world::Quest>> {
    quests_converted(&read_all_text(file)?)
}

/// [`try_load_quests_converted`] over the file's text.
pub(crate) fn quests_converted(text: &str) -> R<Vec<world::Quest>> {
    let gdle_model = deserialize_list(text, "List<Quest>", |e, _| Quest::read(e))?
        .ok_or("the document is null")?;

    let mut results = Vec::new();

    for value in gdle_model.iter().flatten() {
        if let Ok(result) = gdle::try_convert_quest(value) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: GDLELoader.TryLoadSpells
pub fn try_load_spells(file: &Path) -> R<Option<Spells>> {
    load(file, |doc| Spells::read(doc, "Spells"))
}

// ACE: GDLELoader.TryLoadSpellsConverted
pub fn try_load_spells_converted(file: &Path) -> R<Vec<Spell>> {
    spells_converted(&read_all_text(file)?)
}

/// [`try_load_spells_converted`] over the file's text.
pub(crate) fn spells_converted(text: &str) -> R<Vec<Spell>> {
    let gdle_model =
        deserialize(text, |doc| Spells::read(doc, "Spells"))?.ok_or("the document is null")?;
    let table = not_null(&gdle_model.table, "table")?;
    let hash = not_null(&table.spell_base_hash, "spellBaseHash")?;

    let mut results = Vec::new();

    for value in hash {
        // `value.Key` is read before the call, so a null element fails the whole load.
        let value = not_null(value, "spellBaseHash element")?;
        // TryConvert catches a null SpellValue (input.Name) as false.
        let Some(spell) = &value.value else { continue };
        if let Ok(result) = converter::try_convert_spell(value.key, spell) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: GDLELoader.TryLoadRecipe
pub fn try_load_recipe(file: &Path) -> R<Option<Recipe>> {
    load(file, |doc| Recipe::read(doc, "Recipe"))
}

// ACE: GDLELoader.TryLoadRecipeCombined
pub fn try_load_recipe_combined(file: &Path) -> R<Option<RecipeCombined>> {
    load(file, RecipeCombined::read)
}

// ACE: GDLELoader.TryLoadRecipeCombinedInParallel
pub fn try_load_recipe_combined_in_parallel(folder: &Path) -> R<Vec<Option<RecipeCombined>>> {
    let files = json_files(folder)?;

    // DIVERGE: Parallel.ForEach into a ConcurrentBag (unspecified order); files in path order here.
    let mut recipes = Vec::new();
    for file in files {
        if let Ok(result) = try_load_recipe_combined(&file) {
            recipes.push(result);
        }
    }

    Ok(recipes)
}

// ACE: GDLELoader.TryLoadRecipeCombinedConverted
pub fn try_load_recipe_combined_converted(
    folder: &Path,
) -> R<(Vec<world::Recipe>, Vec<world::CookBook>)> {
    let gdle_model = try_load_recipe_combined_in_parallel(folder)?;

    let mut recipes = Vec::new();

    let mut cook_books = Vec::new();

    for value in &gdle_model {
        let value = not_null(value, "recipe document")?;
        // TryConvert(Models.Recipe) catches a null recipe as false.
        let converted = match &value.recipe {
            Some(recipe) => gdle::convert_recipe(recipe),
            None => Err("recipe is null".to_owned()),
        };
        if let Ok(result) = converted {
            for precursor in not_null(&value.precursors, "precursors")? {
                // TryConvert(RecipePrecursor) catches a null precursor as false.
                let Some(precursor) = precursor else { continue };
                let mut result2 = converter::try_convert_precursor(precursor);
                result2.recipe_id = value.key;
                cook_books.push(result2);
            }
            recipes.push(result);
        }
    }

    Ok((recipes, cook_books))
}

// ACE: GDLELoader.TryLoadRecipesConverted
pub fn try_load_recipes_converted(file: &Path) -> R<Vec<world::Recipe>> {
    recipes_converted(&read_all_text(file)?)
}

/// [`try_load_recipes_converted`] over the file's text.
pub(crate) fn recipes_converted(text: &str) -> R<Vec<world::Recipe>> {
    let gdle_model =
        deserialize_list(text, "List<Recipe>", Recipe::read)?.ok_or("the document is null")?;

    let mut results = Vec::new();

    for value in gdle_model.iter().flatten() {
        if let Ok(result) = gdle::convert_recipe(value) {
            results.push(result);
        }
    }

    Ok(results)
}

// ACE: GDLELoader.TryLoadRecipePrecursors
pub fn try_load_recipe_precursors(file: &Path) -> R<Option<Vec<Option<RecipePrecursor>>>> {
    load_list(file, "List<RecipePrecursor>", RecipePrecursor::read)
}

// ACE: GDLELoader.TryLoadRecipePrecursorsConverted
pub fn try_load_recipe_precursors_converted(file: &Path) -> R<Vec<world::CookBook>> {
    recipe_precursors_converted(&read_all_text(file)?)
}

/// [`try_load_recipe_precursors_converted`] over the file's text.
pub(crate) fn recipe_precursors_converted(text: &str) -> R<Vec<world::CookBook>> {
    let gdle_model = deserialize_list(text, "List<RecipePrecursor>", RecipePrecursor::read)?
        .ok_or("the document is null")?;

    let mut results = Vec::new();

    for value in gdle_model.iter().flatten() {
        results.push(converter::try_convert_precursor(value));
    }

    Ok(results)
}

// ACE: GDLELoader.TryLoadRegion
pub fn try_load_region(file: &Path) -> R<Option<Region>> {
    load(file, |doc| Region::read(doc, "Region"))
}

// ACE: GDLELoader.TryLoadTerrainData
pub fn try_load_terrain_data(file: &Path) -> R<Option<Vec<Option<TerrainData>>>> {
    load_list(file, "List<TerrainData>", TerrainData::read)
}

// ACE: GDLELoader.TryLoadWieldedTreasureTable
pub fn try_load_wielded_treasure_table(
    file: &Path,
) -> R<Option<Vec<Option<WieldedTreasureTable>>>> {
    load_list(
        file,
        "List<WieldedTreasureTable>",
        WieldedTreasureTable::read,
    )
}

// ACE: GDLELoader.TryLoadWieldedTreasureTableConverted
pub fn try_load_wielded_treasure_table_converted(file: &Path) -> R<Vec<TreasureWielded>> {
    wielded_treasure_table_converted(&read_all_text(file)?)
}

/// [`try_load_wielded_treasure_table_converted`] over the file's text.
pub(crate) fn wielded_treasure_table_converted(text: &str) -> R<Vec<TreasureWielded>> {
    let gdle_model = deserialize_list(
        text,
        "List<WieldedTreasureTable>",
        WieldedTreasureTable::read,
    )?
    .ok_or("the document is null")?;

    let mut results = Vec::new();

    for value in gdle_model.iter().flatten() {
        if let Ok(result) = converter::try_convert_wielded_treasure(value) {
            results.extend(result);
        }
    }

    Ok(results)
}
