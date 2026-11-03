//! What the server reads from the data files at start beside the world database: the character
//! data, the levels, skills and vitae costs, the templates the world makes things of, the ground;
//! and the files it serves clients' requests for records from.

use std::collections::BTreeMap;
use std::path::Path;

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_script::graph::Graph;
use auberean_script::library::{Library, RECORD};
use yalain_content::{describe, Catalog, LootTable, Props, SpawnTable, Template};
use yalain_world::chargen::BODY_ENTITIES;
use yalain_world::{CharGenRules, StartRules};

/// The install's files a client's requests for records are served from: its portal file, its
/// first cell file and its English file (each absent when it does not open).
///
/// # Errors
/// The install folder does not read.
pub fn install_files(dats: &Path) -> Result<yalain_world::download::InstallFiles, String> {
    let files = install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
    let open = |want: &dyn Fn(&DatName) -> bool| {
        files
            .iter()
            .find(|(n, _)| want(n))
            .and_then(|(_, p)| DatFile::open(p).ok())
    };
    Ok(yalain_world::download::InstallFiles {
        portal: open(&|n| *n == DatName::Portal),
        cell: open(&|n| *n == DatName::Cell(1)),
        english: open(&|n| matches!(n, DatName::Local(l) if l.eq_ignore_ascii_case("english"))),
    })
}

/// The character generation data's saved state.
pub const MATRIX: u32 = 0x7000_0390;

/// The install's portal file and its decoding context.
pub struct Portal {
    pub dat: DatFile,
    pub ctx: Context,
}

impl Portal {
    /// Open an install's portal file.
    ///
    /// # Errors
    /// The install has none, or it does not open.
    pub fn open(dats: &Path) -> Result<Self, String> {
        let files = install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
        let (_, portal) = files
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)
            .ok_or_else(|| format!("no portal data file in {}", dats.display()))?;
        let dat = DatFile::open(&portal).map_err(|e| e.to_string())?;
        let ctx = Context::from_portal(&dat).map_err(|e| e.to_string())?;
        Ok(Self { dat, ctx })
    }

    fn entity(&self, id: u32) -> Option<Entity> {
        let bytes = self.dat.read(id).ok()?;
        auberean_assets::decode(id, &bytes, &self.ctx).ok()
    }

    fn qualities(&self, id: u32) -> Option<Qualities> {
        let bytes = self.dat.read(id).ok()?;
        auberean_assets::decode(id, &bytes, &self.ctx).ok()
    }
}

/// Each race and sex's body look, read from its body entity.
///
/// # Errors
/// A body entity that does not read.
pub fn chargen_rules(p: &Portal) -> Result<CharGenRules, String> {
    let mut bodies = BTreeMap::new();
    for (race, sex, entity) in BODY_ENTITIES {
        let bytes = p
            .dat
            .read(entity)
            .map_err(|e| format!("body {entity:08X}: {e}"))?;
        let e: Entity = auberean_assets::decode(entity, &bytes, &p.ctx)
            .map_err(|e| format!("body {entity:08X}: {e}"))?;
        match e.object {
            Some(look) => {
                bodies.insert((race, sex), look);
            }
            None => tracing::warn!(
                race,
                sex,
                "body {entity:08X} has no look; that race and sex cannot be made"
            ),
        }
    }
    Ok(CharGenRules::with_bodies(bodies))
}

/// Where new characters start and what they start with, from the character generation data (read
/// with the script library's field names).
///
/// # Errors
/// The library or the data does not read.
pub fn start_rules(p: &Portal) -> Result<StartRules, String> {
    let lib = library(p)?;
    let bytes = p
        .dat
        .read(MATRIX)
        .map_err(|e| format!("{MATRIX:08X}: {e}"))?;
    let st: ScriptState = auberean_assets::decode(MATRIX, &bytes, &p.ctx)
        .map_err(|e| format!("{MATRIX:08X}: {e}"))?;
    let g = Graph::decode(&st.graph, &lib).map_err(|e| format!("{MATRIX:08X}: {e}"))?;
    StartRules::from_matrix(&lib, &g)
}

/// The ids of the properties a template's facts are read from, found by name in the data's
/// property list.
#[must_use]
pub fn fact_ids(p: &Portal) -> yalain_content::FactIds {
    let id = |name: &str| {
        p.ctx
            .properties()
            .and_then(|l| l.names.names.iter().find(|(_, n)| n == name))
            .map_or(0, |(v, _)| *v)
    };
    let n = yalain_content::FACT_NAMES;
    yalain_content::FactIds {
        name: id(n[0]),
        description: id(n[1]),
        level: id(n[2]),
        max_health: id(n[3]),
        max_vigor: id(n[4]),
        faction: id(n[5]),
        group: id(n[6]),
        min_level: id(n[7]),
    }
}

/// The character levels, from the level table (read with the script library's field names).
///
/// # Errors
/// The library or the table does not read.
pub fn level_table(p: &Portal) -> Result<yalain_world::levels::LevelTable, String> {
    let lib = library(p)?;
    let id = yalain_world::levels::LEVEL_TABLE;
    let bytes = p.dat.read(id).map_err(|e| format!("{id:08X}: {e}"))?;
    let st: ScriptState =
        auberean_assets::decode(id, &bytes, &p.ctx).map_err(|e| format!("{id:08X}: {e}"))?;
    let g = Graph::decode(&st.graph, &lib).map_err(|e| format!("{id:08X}: {e}"))?;
    yalain_world::levels::LevelTable::from_state(&lib, &g)
}

/// The vitae cost table (the experience to raise vitae by one at full vitae, by level from 1),
/// read with the script library's field names.
///
/// # Errors
/// The library or the table does not read.
pub fn vitae_costs(p: &Portal) -> Result<Vec<u64>, String> {
    let lib = library(p)?;
    let id = yalain_world::death::VITAE_COSTS;
    let bytes = p.dat.read(id).map_err(|e| format!("{id:08X}: {e}"))?;
    let st: ScriptState =
        auberean_assets::decode(id, &bytes, &p.ctx).map_err(|e| format!("{id:08X}: {e}"))?;
    let g = Graph::decode(&st.graph, &lib).map_err(|e| format!("{id:08X}: {e}"))?;
    yalain_world::death::vitae_costs_from_state(&lib, &g)
}

/// The skills' definitions (each skill's saved state, found by its name in the skills'
/// enumeration) and the advancement tables they name, read with the script library's field names.
///
/// # Errors
/// The library or the skills' names do not read.
pub fn skill_table(p: &Portal) -> Result<yalain_world::skills::SkillTable, String> {
    use yalain_world::skills::{SkillTable, SKILL_NAMES};
    let lib = library(p)?;
    let bytes = p
        .dat
        .read(SKILL_NAMES)
        .map_err(|e| format!("{SKILL_NAMES:08X}: {e}"))?;
    let names: auberean_assets::strings::EnumMap =
        auberean_assets::decode(SKILL_NAMES, &bytes, &p.ctx)
            .map_err(|e| format!("{SKILL_NAMES:08X}: {e}"))?;
    let graph = |id: u32| -> Option<Graph> {
        let bytes = p.dat.read(id).ok()?;
        let st: ScriptState = auberean_assets::decode(id, &bytes, &p.ctx).ok()?;
        Graph::decode(&st.graph, &lib).ok()
    };
    Ok(SkillTable::read(&lib, &names.names, &graph))
}

/// Templates the world makes things of beyond the camps' creatures and their loot: the corpse,
/// the starting packs of the races and the book a new character is given.
pub const MORE_TEMPLATES: [u32; 11] = [
    0x4700_0539,
    0x4700_051B,
    0x4700_051C,
    0x4700_051D,
    0x4700_0515,
    0x4700_0516,
    0x4700_0517,
    0x4700_0521,
    0x4700_0522,
    0x4700_0523,
    0x4700_06CD,
];

/// The generator-profile names' enumeration (loot profiles among them).
pub const PROFILE_NAMES: u32 = 0x2300_0100;

/// The templates the world makes things of: every template a camp spawns, every item a loot
/// profile names, the fixed drops of those creatures, and `more` (corpses, store products); each
/// described as the content pack's objects are, with its facts and its properties by name.
///
/// # Errors
/// The profile names do not read.
pub fn catalog(
    p: &Portal,
    spawns: &SpawnTable,
    loot: &LootTable,
    more: &[u32],
) -> Result<(Catalog, Vec<String>), String> {
    let mut c = Catalog::default();
    let bytes = p
        .dat
        .read(PROFILE_NAMES)
        .map_err(|e| format!("{PROFILE_NAMES:08X}: {e}"))?;
    let names: auberean_assets::strings::EnumMap =
        auberean_assets::decode(PROFILE_NAMES, &bytes, &p.ctx)
            .map_err(|e| format!("{PROFILE_NAMES:08X}: {e}"))?;
    c.profiles = names.names.into_iter().collect();
    let ids = fact_ids(p);
    let prop_name = |id: u32| {
        p.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let mut want: Vec<u32> = spawns
        .camps
        .iter()
        .flat_map(|cp| cp.templates.iter().map(|(t, _)| *t))
        .chain(loot.profiles.iter().flat_map(|lp| {
            lp.items
                .iter()
                .map(|(t, _)| *t)
                .chain(lp.extras.iter().map(|(t, _)| *t))
        }))
        .chain(more.iter().copied())
        .collect();
    let mut skipped = Vec::new();
    while let Some(t) = want.pop() {
        if c.templates.contains_key(&t) {
            continue;
        }
        let props = Props::of(t, &|id| p.entity(id), &prop_name);
        let description = match describe(t, &|id| p.entity(id), &|id| p.qualities(id)) {
            Ok(d) => d,
            Err(e) => {
                skipped.push(format!("{t:08X}: {e}"));
                continue;
            }
        };
        if let Some(stats) = yalain_content::CreatureStats::from_props(&props) {
            want.extend(stats.trophies.iter().map(|(x, _)| *x));
        }
        let icon = icon_layers(p, description.look);
        if !icon.is_empty() {
            c.icons.insert(t, icon);
        }
        c.insert(Template {
            id: t,
            description,
            facts: yalain_content::facts(t, &|id| p.entity(id), &ids),
            props,
        });
    }
    Ok((c, skipped))
}

/// The templates of the weapons the catalog's creatures wield, by the names their templates give
/// them: a name the world database lists, its template there; any other name of a script package,
/// the entity made of that package (the lowest of several). A name of neither is left out.
///
/// # Errors
/// The library does not read.
pub fn creature_weapons(
    p: &Portal,
    c: &Catalog,
    listed: &BTreeMap<String, u32>,
) -> Result<std::collections::HashMap<String, u32>, String> {
    let lib = library(p)?;
    let mut by_package: BTreeMap<u32, String> = BTreeMap::new();
    for t in c.templates.values() {
        let Some(stats) = yalain_content::CreatureStats::from_props(&t.props) else {
            continue;
        };
        for (name, _) in yalain_world::arms::held_names(&stats) {
            if listed.contains_key(&name) {
                continue;
            }
            if let Some(pkg) = lib.package_id(&name) {
                by_package.insert(pkg, name);
            }
        }
    }
    let mut out: std::collections::HashMap<String, u32> =
        listed.iter().map(|(n, t)| (n.clone(), *t)).collect();
    if by_package.is_empty() {
        return Ok(out);
    }
    let ids: Vec<u32> = p
        .dat
        .entries_in(0x4700_0000, 0x47FF_FFFF)
        .map(|e| e.id)
        .collect();
    for id in ids {
        let Some(e) = p.entity(id) else { continue };
        if e.kind != Some(ENTITY_OF_PACKAGE) {
            continue;
        }
        if let Some(name) = e.object.and_then(|pkg| by_package.get(&pkg)) {
            out.entry(name.clone()).or_insert(id);
        }
    }
    Ok(out)
}

/// The kind of an entity made of a script package.
const ENTITY_OF_PACKAGE: u32 = 0x4000_0002;

/// The script library. A read or decode that fails is checked against the file and read once
/// more; a second read that succeeds is warned of.
fn library(p: &Portal) -> Result<Library, String> {
    let lib = p
        .dat
        .read_checked(RECORD, Library::from_record)
        .map_err(|e| format!("the script library: {e}"))?;
    if let Some(r) = lib.retried {
        tracing::warn!("the script library: {r}");
    }
    Ok(lib.value)
}

/// A saved state's object graph.
fn state_graph(p: &Portal, lib: &Library, id: u32) -> Option<Graph> {
    let bytes = p.dat.read(id).ok()?;
    let st: ScriptState = auberean_assets::decode(id, &bytes, &p.ctx).ok()?;
    Graph::decode(&st.graph, lib).ok()
}

/// The effects' definitions of these saved states, and of the effects they add.
///
/// # Errors
/// The library does not read.
pub fn effect_table(
    p: &Portal,
    states: &[u32],
) -> Result<yalain_world::effect_defs::EffectTable, String> {
    let lib = library(p)?;
    Ok(yalain_world::effect_defs::EffectTable::read(
        &lib,
        states,
        &|id| state_graph(p, &lib, id),
    ))
}

/// The effects each item template's profile lists, read from the effects' saved states (the
/// singleton of the library's package each effect is named by); an effect the library does not
/// hold is left out.
///
/// # Errors
/// The library does not read.
pub fn item_effects(
    p: &Portal,
    catalog: &Catalog,
) -> Result<std::collections::HashMap<u32, Vec<yalain_world::profile::ItemEffect>>, String> {
    use yalain_world::effect_defs::EffectDef;
    use yalain_world::profile::{item_effects, EffectShown};
    let lib = library(p)?;
    let mut known: BTreeMap<String, Option<EffectShown>> = BTreeMap::new();
    let mut shown = |name: &str| -> Option<EffectShown> {
        *known.entry(name.to_string()).or_insert_with(|| {
            let state = 0x7000_0000 | lib.package_id(name)?;
            let g = state_graph(p, &lib, state)?;
            EffectShown::of(&EffectDef::from_state(state, &lib, &g).ok()?)
        })
    };
    let mut out = std::collections::HashMap::new();
    for (id, t) in &catalog.templates {
        let list = item_effects(&t.props, &mut shown);
        if !list.is_empty() {
            out.insert(*id, list);
        }
    }
    Ok(out)
}

/// The mounts: the mount effects among the install's saved states, the mount table's looks and the
/// bodies' own appearance (the items that put them on are noted from the items' uses).
///
/// # Errors
/// The library does not read.
pub fn mounts(p: &Portal, bodies: &[u32]) -> Result<yalain_world::mounts::Mounts, String> {
    let lib = library(p)?;
    let states: Vec<(u32, u32)> = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|id| {
            let st: ScriptState =
                auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok()?;
            Some((id, st.package))
        })
        .collect();
    Ok(yalain_world::mounts::Mounts::read(
        &lib,
        &states,
        &|id| state_graph(p, &lib, id),
        &|id| p.dat.read(id).ok(),
        bodies,
    ))
}

/// The interaction tables the item templates name (what each item may be used on).
///
/// # Errors
/// The library does not read.
pub fn interactions(
    p: &Portal,
    catalog: &Catalog,
) -> Result<yalain_world::usable::interactions::Interactions, String> {
    let lib = library(p)?;
    let mut states: Vec<u32> = catalog
        .templates
        .values()
        .filter_map(|t| t.props.word(yalain_world::usable::interactions::TABLE))
        .collect();
    states.sort_unstable();
    states.dedup();
    let templates: Vec<(u32, u32)> = catalog
        .templates
        .values()
        .map(|t| (t.id, t.description.package))
        .collect();
    Ok(yalain_world::usable::interactions::Interactions::read(
        &lib,
        &states,
        &|id| state_graph(p, &lib, id),
        &templates,
    ))
}

/// The effects each item template's use puts on its user, and those it puts on whoever wears or
/// wields it: each name the template gives resolved to the saved state of its effect enumeration
/// value, else of its package (the package's one saved state among the effects', else its
/// singleton).
///
/// # Errors
/// The library does not read.
pub fn item_uses(
    p: &Portal,
    catalog: &Catalog,
) -> Result<
    (
        yalain_world::item_use::ItemUses,
        yalain_world::item_use::ItemUses,
    ),
    String,
> {
    use yalain_world::item_use::{read_named, USED, WORN};
    let lib = library(p)?;
    let ids: Vec<u32> = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect();
    let states: Vec<(u32, u32, Option<u32>)> = ids
        .into_iter()
        .filter_map(|id| {
            let st: ScriptState =
                auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok()?;
            let value = Graph::decode(&st.graph, &lib)
                .ok()
                .and_then(|g| yalain_world::effect_defs::EffectDef::enum_value_of(&lib, &g));
            Some((id, st.package, value))
        })
        .collect();
    let templates = || catalog.templates.iter().map(|(id, t)| (*id, &t.props));
    Ok((
        read_named(&lib, states.iter().copied(), templates(), USED),
        read_named(&lib, states.iter().copied(), templates(), WORN),
    ))
}

/// The enumeration of quests: each quest's id by its name.
pub const QUEST_NAMES: u32 = 0x2300_0090;

/// The quests' ids by their names.
///
/// # Errors
/// The enumeration does not read.
pub fn quest_ids(p: &Portal) -> Result<std::collections::HashMap<String, u32>, String> {
    let bytes = p
        .dat
        .read(QUEST_NAMES)
        .map_err(|e| format!("{QUEST_NAMES:08X}: {e}"))?;
    let names: auberean_assets::strings::EnumMap =
        auberean_assets::decode(QUEST_NAMES, &bytes, &p.ctx)
            .map_err(|e| format!("{QUEST_NAMES:08X}: {e}"))?;
    Ok(names.names.into_iter().map(|(v, n)| (n, v)).collect())
}

/// The dye plants' ripe looks: each species' plant item's look, by the item.
#[must_use]
pub fn plant_looks(p: &Portal, catalog: &Catalog) -> yalain_world::plants::Looks {
    yalain_world::plants::SPECIES
        .iter()
        .filter_map(|sp| {
            let look = catalog.get(sp.harvest)?.description.look;
            let v: auberean_assets::visual::VisualDesc =
                auberean_assets::decode(look, &p.dat.read(look).ok()?, &p.ctx).ok()?;
            Some((sp.harvest, yalain_world::plants::look_of(&v)))
        })
        .collect()
}

/// The packages of the world's objects that a player uses by walking into them: those derived
/// from the kinds the script library makes so (the world's portals and vaults).
///
/// # Errors
/// The library does not read.
pub fn walk_in_packages(
    p: &Portal,
    content: &yalain_content::ContentPack,
) -> Result<std::collections::HashSet<u32>, String> {
    let lib = library(p)?;
    let bases: Vec<u32> = yalain_world::places::USED_BY_WALKING_INTO
        .iter()
        .filter_map(|n| lib.package_id(n))
        .collect();
    Ok(content
        .objects
        .iter()
        .map(|o| o.description.package)
        .filter(|pkg| bases.iter().any(|b| lib.derives_from(*pkg, *b)))
        .collect())
}

/// The packages of the world's objects derived from any of the script library's packages named.
///
/// # Errors
/// The library does not read.
pub fn packages_deriving_from(
    p: &Portal,
    content: &yalain_content::ContentPack,
    names: &[&str],
) -> Result<std::collections::HashSet<u32>, String> {
    let lib = library(p)?;
    let bases: Vec<u32> = names.iter().filter_map(|n| lib.package_id(n)).collect();
    Ok(content
        .objects
        .iter()
        .map(|o| o.description.package)
        .filter(|pkg| bases.iter().any(|b| lib.derives_from(*pkg, *b)))
        .collect())
}

/// The packages of the world's doors (those derived from the script library's door), each with
/// the kind of door its name makes it; and those whose doors let creatures through their locks.
///
/// # Errors
/// The library does not read.
pub fn door_packages(
    p: &Portal,
    content: &yalain_content::ContentPack,
) -> Result<
    (
        std::collections::HashMap<u32, yalain_world::usable::doors::Kind>,
        std::collections::HashSet<u32>,
    ),
    String,
> {
    use yalain_world::usable::doors;
    let lib = library(p)?;
    let Some(door) = lib.package_id(doors::PACKAGE) else {
        return Ok(Default::default());
    };
    let packages: std::collections::HashSet<u32> = content
        .objects
        .iter()
        .map(|o| o.description.package)
        .filter(|pkg| lib.derives_from(*pkg, door))
        .collect();
    let kinds = packages
        .iter()
        .map(|&pkg| (pkg, doors::Kind::of_package(&lib.package_name(pkg))))
        .collect();
    let through = packages
        .into_iter()
        .filter(|&pkg| doors::package_lets_creatures_through(&lib.package_name(pkg)))
        .collect();
    Ok((kinds, through))
}

/// The packages of the world's chests (those derived from the script library's chest).
///
/// # Errors
/// The library does not read.
pub fn chest_packages(
    p: &Portal,
    content: &yalain_content::ContentPack,
) -> Result<std::collections::HashSet<u32>, String> {
    let lib = library(p)?;
    let Some(chest) = lib.package_id(yalain_world::usable::chests::PACKAGE) else {
        return Ok(std::collections::HashSet::new());
    };
    Ok(content
        .objects
        .iter()
        .map(|o| o.description.package)
        .filter(|pkg| lib.derives_from(*pkg, chest))
        .collect())
}

/// The packages of the catalog's templates that derive from any of these script packages.
///
/// # Errors
/// The library does not read.
pub fn catalog_packages_from(
    p: &Portal,
    catalog: &Catalog,
    bases: &[&str],
) -> Result<std::collections::HashSet<u32>, String> {
    let lib = library(p)?;
    let bases: Vec<u32> = bases.iter().filter_map(|n| lib.package_id(n)).collect();
    Ok(catalog
        .templates
        .values()
        .map(|t| t.description.package)
        .filter(|pkg| bases.iter().any(|b| lib.derives_from(*pkg, *b)))
        .collect())
}

/// The packages of the catalog's treasure equipment (not its instruments) and treasure potions,
/// and each treasure potion effect's name table and color.
///
/// # Errors
/// The library does not read.
pub fn treasure(p: &Portal, catalog: &Catalog) -> Result<yalain_world::treasure::Kinds, String> {
    use yalain_world::treasure;
    let lib = library(p)?;
    let instruments = catalog_packages_from(p, catalog, &treasure::INSTRUMENTS)?;
    let equipment = catalog_packages_from(p, catalog, &treasure::EQUIPMENT)?
        .into_iter()
        .filter(|pkg| !instruments.contains(pkg))
        .collect();
    let potion_looks = treasure::POTION_EFFECTS
        .iter()
        .flatten()
        .filter_map(|&state| {
            let g = state_graph(p, &lib, state)?;
            let (_, table) = yalain_world::effect_defs::EffectDef::from_state(state, &lib, &g)
                .ok()?
                .name?;
            Some((state, (table, treasure::potion_color(&lib, &g)?)))
        })
        .collect();
    Ok(treasure::Kinds {
        equipment,
        potions: catalog_packages_from(p, catalog, &treasure::POTIONS)?,
        potion_looks,
    })
}

/// The packages of the catalog's templates whose items are destroyed on use by their kind.
///
/// # Errors
/// The library does not read.
pub fn destroyed_on_use(
    p: &Portal,
    catalog: &Catalog,
) -> Result<std::collections::HashSet<u32>, String> {
    let lib = library(p)?;
    Ok(catalog
        .templates
        .values()
        .map(|t| t.description.package)
        .filter(|pkg| {
            yalain_world::profile::destroyed_on_use_kind(
                *pkg,
                |n| lib.package_id(n),
                |a, b| lib.derives_from(a, b),
            )
        })
        .collect())
}

/// The craft skills and recipes a new character starts with, and the recipes `more` names (the
/// ones effects grant), read from their saved states.
///
/// # Errors
/// The library does not read.
pub fn crafting(
    p: &Portal,
    start: &yalain_content::CraftStart,
    more: &[u32],
) -> Result<yalain_world::craft::Crafting, String> {
    let lib = library(p)?;
    Ok(yalain_world::craft::Crafting::read(
        &lib,
        &start.skills,
        more,
        &start.recipes,
        &|id| state_graph(p, &lib, id),
    ))
}

/// The bodies of the catalog's templates whose looks give one.
pub fn bodies(p: &Portal, catalog: &Catalog) -> BTreeMap<u32, [f32; 2]> {
    let visual = |did: u32| auberean_assets::decode(did, &p.dat.read(did).ok()?, &p.ctx).ok();
    let setup = |did: u32| {
        let s: auberean_assets::setup::Setup =
            auberean_assets::decode(did, &p.dat.read(did).ok()?, &p.ctx).ok()?;
        Some((s.radius, s.height))
    };
    catalog
        .templates
        .values()
        .filter_map(|t| {
            yalain_content::look_body(t.description.look, &visual, &setup).map(|b| (t.id, b))
        })
        .collect()
}

/// The brokers among the world's placed objects: each template whose properties name a store
/// group (its catalog) and where it stands.
pub fn brokers(
    p: &Portal,
    content: &yalain_content::ContentPack,
) -> BTreeMap<u32, yalain_world::market::Broker> {
    let prop_name = |id: u32| {
        p.ctx
            .properties()
            .and_then(|l| l.names.name(id))
            .map(str::to_string)
    };
    let mut out = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for o in &content.objects {
        if !seen.insert(o.template) {
            continue;
        }
        let props = Props::of(o.template, &|id| p.entity(id), &prop_name);
        let Some(catalog) = props.word("StoreGroup").filter(|c| *c != 0) else {
            continue;
        };
        let location = match props.get("StoreLocation") {
            Some(yalain_content::Prop::Str(s, t)) => (*s, *t),
            _ => (0, 0),
        };
        out.insert(
            o.template,
            yalain_world::market::Broker { catalog, location },
        );
    }
    out
}

/// The icon a look gives (its visual descriptions' icon layers).
pub fn icon_layers(p: &Portal, look: u32) -> Vec<[u32; 3]> {
    yalain_content::icon_layers(look, &|did| {
        auberean_assets::decode(did, &p.dat.read(did).ok()?, &p.ctx).ok()
    })
}

/// The ground outdoors, from the land of the blocks the camps stand in and their neighbors; and
/// what each interior cell of the world sees.
pub struct LandGround {
    land: std::sync::Arc<auberean_physics::land::Land>,
    blocks: usize,
    sees: std::collections::HashMap<u32, Vec<u32>>,
}

impl std::fmt::Debug for LandGround {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "LandGround({} blocks, {} interior cells)",
            self.blocks,
            self.sees.len()
        )
    }
}

impl yalain_world::Ground for LandGround {
    fn height(&self, cell: u32, origin: [f32; 3]) -> Option<f32> {
        #[allow(clippy::cast_possible_truncation)]
        let block = dereth_primitives::LandblockId((cell >> 16) as u16);
        self.land
            .ground(
                block,
                dereth_primitives::Vec3::new(origin[0], origin[1], origin[2]),
            )
            .map(|(z, _)| z)
    }

    fn sees(&self, cell: u32) -> Option<&[u32]> {
        self.sees.get(&cell).map(Vec::as_slice)
    }
}

/// The land under the outdoor camps: every landblock a camp stands in, and those around it; and
/// the walls the creatures meet on that land, read further a block at a time as they move, with
/// the body of every template whose look gives one.
///
/// # Errors
/// The install's world records do not open.
pub fn land_ground(
    dats: &Path,
    spawns: &SpawnTable,
    catalog: &Catalog,
) -> Result<(LandGround, yalain_world::walls::Walls), String> {
    let files = install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
    let open = |want: DatName| -> Result<DatFile, String> {
        let p = files
            .iter()
            .find(|(n, _)| *n == want)
            .map(|(_, p)| p.clone())
            .ok_or_else(|| format!("no {want:?} file in {}", dats.display()))?;
        DatFile::open(&p).map_err(|e| e.to_string())
    };
    let portal = open(DatName::Portal)?;
    let cell = open(DatName::Cell(1))?;
    let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = auberean_world::records::WorldRecords::new(portal, cell, None, ctx);
    let region = records.region().map_err(|e| format!("{e:?}"))?;
    let land = auberean_physics::land::Land::new();
    let mut trees = auberean_physics::mesh::Trees::default();
    let mut blocks = std::collections::BTreeSet::new();
    for c in spawns.camps.iter().filter(|c| c.cell & 0xFFFF < 0x100) {
        let (x, y) = (
            i32::from(c.cell.to_be_bytes()[0]),
            i32::from(c.cell.to_be_bytes()[1]),
        );
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let (Ok(bx), Ok(by)) = (u8::try_from(x + dx), u8::try_from(y + dy)) {
                    blocks.insert((bx, by));
                }
            }
        }
    }
    let mut n = 0;
    for (x, y) in blocks {
        if land.add_block(&records, &region, &mut trees, x, y) {
            n += 1;
        }
    }
    // Every interior cell of the world, with the cells it sees (about a second's reading).
    let mut sees = std::collections::HashMap::new();
    for place in 0..=0xFFFF_u32 {
        let [_, _, bx, by] = place.to_be_bytes();
        let (x, y) = (i32::from(bx), i32::from(by));
        let Some(info) = records.landblock_info(x, y) else {
            continue;
        };
        for i in 0..info.cell_count {
            let Ok(low) = u16::try_from(0x100 + i) else {
                break;
            };
            if let Some(c) = records.cell(x, y, low) {
                let whole = |l: u16| place << 16 | u32::from(l);
                sees.insert(whole(low), c.visible.iter().map(|l| whole(*l)).collect());
            }
        }
    }
    let land = std::sync::Arc::new(land);
    let mut world = auberean_physics::world::World::new();
    if let Some(list) = records.get::<auberean_assets::property::PropertyList>(0x3400_0000) {
        world.set_property_list(list);
    }
    let walkers = walkers(&records, catalog);
    let blocks = TimedBlocks {
        inner: yalain_world::walls::RecordBlocks::new(records, region),
        spent: std::time::Duration::ZERO,
    };
    let mut walls = yalain_world::walls::Walls::new(land.clone(), world, Box::new(blocks));
    walls.walkers = walkers;
    Ok((
        LandGround {
            land,
            blocks: n,
            sees,
        },
        walls,
    ))
}

/// The bodies of the templates whose looks give one: their setups' collision spheres and step
/// heights, at the looks' scale.
fn walkers(
    records: &auberean_world::records::WorldRecords,
    catalog: &Catalog,
) -> std::collections::HashMap<u32, yalain_world::walls::Walker> {
    catalog
        .templates
        .values()
        .filter_map(|t| {
            Some((
                t.id,
                yalain_world::walls::walker(records, t.description.look)?,
            ))
        })
        .collect()
}

/// The blocks read for the walls, with the time their reading took (logged block by block).
struct TimedBlocks {
    inner: yalain_world::walls::RecordBlocks,
    spent: std::time::Duration,
}

impl yalain_world::walls::Blocks for TimedBlocks {
    fn load(&mut self, land: &auberean_physics::land::Land, x: u8, y: u8) -> bool {
        let started = std::time::Instant::now();
        let had = self.inner.load(land, x, y);
        let took = started.elapsed();
        self.spent += took;
        if had {
            tracing::debug!(
                "the walls of block {x:02X}{y:02X} read in {:.1} ms ({:?} so far, {:.2} s)",
                took.as_secs_f64() * 1000.0,
                self.inner,
                self.spent.as_secs_f64()
            );
        }
        had
    }
}
