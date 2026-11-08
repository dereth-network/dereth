//! What the server reads from the data files at start beside the world database: the character
//! data, the levels, skills and vitae costs, the templates the world makes things of, the ground;
//! and the files it serves clients' requests for records from.

use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::visual::VisualDesc;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::overlay::WorldOverlay;
use auberean_dat::DatFile;
use auberean_script::graph::Graph;
use auberean_script::library::{Library, RECORD};
use yalain_content::{describe, Catalog, LootTable, Props, SpawnTable, Template};
use yalain_world::chargen::BODY_ENTITIES;
use yalain_world::{CharGenRules, StartRules};

/// The install's files a client's requests for records are served from: its portal file, its
/// first cell file and its English file (each absent when it does not open).
#[must_use]
pub fn install_files(p: &Portal) -> yalain_world::download::InstallFiles {
    yalain_world::download::InstallFiles {
        portal: Some(p.dat.clone()),
        cell: p.cell().cloned(),
        english: p.english().cloned(),
    }
}

/// The character generation data's saved state.
pub const MATRIX: u32 = 0x7000_0390;

/// The records a start has read, each decoded once however many tables ask.
struct Kept<T>(RefCell<HashMap<u32, Option<T>>>);

impl<T> Default for Kept<T> {
    fn default() -> Self {
        Self(RefCell::default())
    }
}

impl<T: Clone> Kept<T> {
    fn get(&self, id: u32, read: impl FnOnce() -> Option<T>) -> Option<T> {
        if let Some(v) = self.0.borrow().get(&id) {
            return v.clone();
        }
        let v = read();
        self.0.borrow_mut().insert(id, v.clone());
        v
    }
}

/// The install's portal file, its decoding context and its script library; and the other files of
/// the install, which what needs them shares.
pub struct Portal {
    pub dat: DatFile,
    pub ctx: Context,
    /// What the install holds, for the files opened when they are first asked for.
    files: Vec<(DatName, PathBuf)>,
    /// The world's overlay, which every file is read through.
    overlay: Option<WorldOverlay>,
    cell: OnceCell<Option<DatFile>>,
    english: OnceCell<Option<DatFile>>,
    entities: Kept<Entity>,
    qualities: Kept<Qualities>,
    visuals: Kept<VisualDesc>,
    /// What a body takes of each setup: its radius and its height.
    bodies: Kept<(f32, f32)>,
    /// Read at its first use and shared by everything the start reads with it after that: it is a
    /// large record, and every table of the start needs its field names.
    library: OnceCell<Library>,
    /// Read at its first use and shared likewise: reading them all takes most of what a start reads
    /// after the library.
    saved_states: OnceCell<Vec<SavedState>>,
}

/// What the tables the start reads need of a saved state: its record, its package, the value of
/// the enumeration it fills, when it fills one, and the quest it defines, when it defines one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SavedState {
    pub id: u32,
    pub package: u32,
    pub value: Option<u32>,
    pub quest: Option<u32>,
}

impl Portal {
    /// Open an install's portal file, with `overlay` over each of its files.
    ///
    /// # Errors
    /// The install has none, it does not open, or the overlay was made against other files: every
    /// file it lies over is checked now, so a world is never served from the installed files in
    /// place of its own.
    pub fn open(dats: &Path, overlay: Option<WorldOverlay>) -> Result<Self, String> {
        let files = install_dats(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
        let portal = files
            .iter()
            .find(|(n, _)| *n == DatName::Portal)
            .map(|(_, p)| p)
            .ok_or_else(|| format!("no portal data file in {}", dats.display()))?;
        let dat = DatFile::open(portal).map_err(|e| e.to_string())?;
        let Some(world) = overlay else {
            return Self::over(dat, files, None);
        };
        let through = |file: DatFile| {
            world
                .over(file)
                .map_err(|e| format!("the world's overlay: {e}"))
        };
        for (_, path) in &files {
            through(DatFile::open(path).map_err(|e| format!("{}: {e}", path.display()))?)?;
        }
        let dat = through(dat)?;
        Self::over(dat, files, Some(world))
    }

    /// The portal file already opened, with the install's other files as `files` lists them.
    fn over(
        dat: DatFile,
        files: Vec<(DatName, PathBuf)>,
        overlay: Option<WorldOverlay>,
    ) -> Result<Self, String> {
        let ctx = Context::from_portal(&dat).map_err(|e| e.to_string())?;
        Ok(Self {
            dat,
            ctx,
            files,
            overlay,
            cell: OnceCell::new(),
            english: OnceCell::new(),
            entities: Kept::default(),
            qualities: Kept::default(),
            visuals: Kept::default(),
            bodies: Kept::default(),
            library: OnceCell::new(),
            saved_states: OnceCell::new(),
        })
    }

    fn open_other(&self, want: &dyn Fn(&DatName) -> bool) -> Option<DatFile> {
        let (_, path) = self.files.iter().find(|(n, _)| want(n))?;
        let file = DatFile::open(path).ok()?;
        match &self.overlay {
            Some(world) => world.over(file).ok(),
            None => Some(file),
        }
    }

    /// Whether the files are read through a world's overlay.
    #[must_use]
    pub const fn has_overlay(&self) -> bool {
        self.overlay.is_some()
    }

    /// The install's first cell file, when it opens.
    #[must_use]
    pub fn cell(&self) -> Option<&DatFile> {
        self.cell
            .get_or_init(|| self.open_other(&|n| *n == DatName::Cell(1)))
            .as_ref()
    }

    /// The install's English file, when it opens.
    #[must_use]
    pub fn english(&self) -> Option<&DatFile> {
        self.english
            .get_or_init(|| {
                self.open_other(
                    &|n| matches!(n, DatName::Local(l) if l.eq_ignore_ascii_case("english")),
                )
            })
            .as_ref()
    }

    fn decoded<T: auberean_assets::Decode>(&self, id: u32) -> Option<T> {
        let bytes = self.dat.read(id).ok()?;
        auberean_assets::decode(id, &bytes, &self.ctx).ok()
    }

    fn entity(&self, id: u32) -> Option<Entity> {
        self.entities.get(id, || self.decoded(id))
    }

    fn qualities(&self, id: u32) -> Option<Qualities> {
        self.qualities.get(id, || self.decoded(id))
    }

    fn visual(&self, id: u32) -> Option<VisualDesc> {
        self.visuals.get(id, || self.decoded(id))
    }

    fn appearance(&self, id: u32) -> Option<auberean_assets::visual::Appearance> {
        self.decoded(id)
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
    let g = Graph::decode(&st.graph, lib).map_err(|e| format!("{MATRIX:08X}: {e}"))?;
    StartRules::from_matrix(lib, &g)
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
    let g = Graph::decode(&st.graph, lib).map_err(|e| format!("{id:08X}: {e}"))?;
    yalain_world::levels::LevelTable::from_state(lib, &g)
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
    let g = Graph::decode(&st.graph, lib).map_err(|e| format!("{id:08X}: {e}"))?;
    yalain_world::death::vitae_costs_from_state(lib, &g)
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
        Graph::decode(&st.graph, lib).ok()
    };
    Ok(SkillTable::read(lib, &names.names, &graph))
}

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
        let class = Props::class_of(t, &|id| p.entity(id)).map_or_else(Vec::new, |c| {
            library(p).map_or_else(
                |_| Vec::new(),
                |lib| yalain_world_db::install::class_properties(lib, c),
            )
        });
        let props = Props::of(t, &|id| p.entity(id), &prop_name)
            .with_places_of(p.qualities(yalain_content::qualities_id(t)).as_ref())
            .with_worn_of(t)
            .with_take_gate_of(t)
            .with_class(&class);
        let description = match describe(t, &|id| p.entity(id), &|id| p.qualities(id)) {
            Ok(d) => d,
            Err(e) => {
                skipped.push(format!("{t:08X}: {e}"));
                continue;
            }
        };
        want.extend(yalain_world::makes::by_a_template(&props));
        let icon = icon_layers(p, description.look);
        if !icon.is_empty() {
            c.icons.insert(t, icon);
        }
        let tables = yalain_world::equip::dye_table(&props)
            .into_iter()
            .chain([yalain_world::inventory::POTION_COLOR.0])
            .chain(yalain_content::treasure_looks::table_of(t));
        for table in tables {
            c.icon_tables
                .entry(table)
                .or_insert_with(|| yalain_content::icon_tables(table, &|did| p.appearance(did)));
        }
        c.insert(Template {
            id: t,
            description,
            facts: yalain_content::facts(t, &|id| p.entity(id), &ids).named_by(&props),
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

/// The script library, read once. A read or decode that fails is checked against the file and read
/// once more; a second read that succeeds is warned of.
///
/// # Errors
/// The library does not read.
pub fn library(p: &Portal) -> Result<&Library, String> {
    if let Some(lib) = p.library.get() {
        return Ok(lib);
    }
    let lib = p
        .dat
        .read_checked(RECORD, Library::from_record)
        .map_err(|e| format!("the script library: {e}"))?;
    if let Some(r) = lib.retried {
        tracing::warn!("the script library: {r}");
    }
    Ok(p.library.get_or_init(|| lib.value))
}

/// The install's saved states, read once.
///
/// # Errors
/// The library does not read.
pub fn saved_states(p: &Portal) -> Result<&[SavedState], String> {
    if let Some(states) = p.saved_states.get() {
        return Ok(states);
    }
    let lib = library(p)?;
    let states = p
        .dat
        .entries_in(0x6F00_0000, 0x6FFF_FFFF)
        .map(|e| e.id)
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|id| {
            let st: ScriptState =
                auberean_assets::decode(id, &p.dat.read(id).ok()?, &p.ctx).ok()?;
            let graph = Graph::decode(&st.graph, lib).ok();
            let value = graph
                .as_ref()
                .and_then(|g| yalain_world::effect_defs::EffectDef::enum_value_of(lib, g));
            let quest = graph
                .as_ref()
                .and_then(|g| yalain_world_db::install::quest_of_state(lib, g));
            Some(SavedState {
                id,
                package: st.package,
                value,
                quest,
            })
        })
        .collect();
    Ok(p.saved_states.get_or_init(|| states))
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
        lib,
        states,
        &|id| state_graph(p, lib, id),
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
            let g = state_graph(p, lib, state)?;
            EffectShown::of(&EffectDef::from_state(state, lib, &g).ok()?)
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
    let states: Vec<(u32, u32)> = saved_states(p)?.iter().map(|s| (s.id, s.package)).collect();
    Ok(yalain_world::mounts::Mounts::read(
        lib,
        &states,
        &|id| state_graph(p, lib, id),
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
        lib,
        &states,
        &|id| state_graph(p, lib, id),
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
        yalain_world::item_use::ItemUses,
    ),
    String,
> {
    use yalain_world::item_use::{read_named, DIED, USED, WORN};
    let lib = library(p)?;
    let states: Vec<(u32, u32, Option<u32>)> = saved_states(p)?
        .iter()
        .map(|s| (s.id, s.package, s.value))
        .collect();
    let templates = || catalog.templates.iter().map(|(id, t)| (*id, &t.props));
    Ok((
        read_named(lib, states.iter().copied(), templates(), USED),
        read_named(lib, states.iter().copied(), templates(), WORN),
        read_named(lib, states.iter().copied(), templates(), DIED),
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
            let g = state_graph(p, lib, state)?;
            let (_, table) = yalain_world::effect_defs::EffectDef::from_state(state, lib, &g)
                .ok()?
                .name?;
            Some((state, (table, treasure::potion_color(lib, &g)?)))
        })
        .collect();
    Ok(treasure::Kinds {
        equipment,
        jewelry: catalog_packages_from(p, catalog, &treasure::JEWELRY)?,
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
        lib,
        &start.skills,
        more,
        &start.recipes,
        &|id| state_graph(p, lib, id),
    ))
}

/// The recipes' ingredient classes made the catalog's templates that derive from them.
///
/// # Errors
/// The library does not read.
pub fn resolve_ingredient_classes(
    p: &Portal,
    craft: &mut yalain_world::craft::Crafting,
    catalog: &Catalog,
) -> Result<(), String> {
    let lib = library(p)?;
    let templates: Vec<(u32, u32)> = catalog
        .templates
        .values()
        .map(|t| (t.id, t.description.package))
        .collect();
    craft.resolve_classes(&templates, &|package, class| {
        lib.derives_from(package, class)
    });
    Ok(())
}

/// The bodies of the catalog's templates whose looks give one.
pub fn bodies(p: &Portal, catalog: &Catalog) -> BTreeMap<u32, [f32; 2]> {
    let visual = |did: u32| p.visual(did);
    let setup = |did: u32| {
        p.bodies.get(did, || {
            let s: auberean_assets::setup::Setup = p.decoded(did)?;
            Some((s.radius, s.height))
        })
    };
    catalog
        .templates
        .values()
        .filter_map(|t| {
            yalain_content::look_body(t.description.look, &visual, &setup).map(|b| (t.id, b))
        })
        .collect()
}

/// What the catalog's templates are wielded as, by the script classes they derive from.
///
/// # Errors
/// The library does not read.
pub fn implements(p: &Portal, catalog: &Catalog) -> Result<BTreeMap<u32, u32>, String> {
    let lib = library(p)?;
    Ok(yalain_world::skill_use::implements_of_catalog(
        catalog,
        |package, class| {
            lib.package_id(class)
                .is_some_and(|base| lib.derives_from(package, base))
        },
    ))
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
    yalain_content::icon_layers(look, &|did| p.visual(did))
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
    p: &Portal,
    spawns: &SpawnTable,
    catalog: &Catalog,
) -> Result<(LandGround, yalain_world::walls::Walls), String> {
    let cell = p.cell().ok_or("no first cell file in the install")?;
    let records = auberean_world::records::WorldRecords::new(
        p.dat.clone(),
        cell.clone(),
        None,
        p.ctx.clone(),
    );
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
    let mut near = auberean_physics::land::NearBlocks::default();
    let mut n = 0;
    for (x, y) in blocks {
        if land.add_block_near(&records, &region, &mut trees, &mut near, x, y) {
            n += 1;
        }
    }
    drop(near);
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
        .filter_map(|t| Some((t.id, yalain_world::walls::walker(records, &t.description)?)))
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

    fn stand(
        &mut self,
        land: &auberean_physics::land::Land,
        world: &mut auberean_physics::world::World,
        row: &yalain_world::walls::Standing,
    ) -> Option<auberean_physics::collision::manager::BodyKey> {
        self.inner.stand(land, world, row)
    }
}

#[cfg(all(test, feature = "live"))]
mod tests {
    use super::*;
    use auberean_dat::checked::{Exact, ReadAt};
    use auberean_dat::locate::role_file;
    use auberean_dat::DatKind;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};

    /// A world's folder, `key`, its overlay holding `build`'s records.
    fn world_with_overlay(
        name: &str,
        key: &str,
        build: impl FnOnce(&dereth_dat::overlay::OverlayDir),
    ) -> PathBuf {
        let world = std::env::temp_dir().join(format!(
            "yalain-world-overlay-{name}-{}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&world).ok();
        std::fs::create_dir_all(&world).expect("the world's folder");
        std::fs::write(
            world.join("world.toml"),
            format!(
                "format_version = 1\nkey = \"{key}\"\ngame = \"ac2\"\nera = \"eor\"\npacks = []\n"
            ),
        )
        .expect("the world file");
        let folder = world.join(auberean_dat::overlay::WORLD_FOLDER);
        build(&dereth_dat::overlay::OverlayDir::new(&folder).expect("the folder"));
        world
    }

    /// An install's file.
    fn file_of(want: &DatName) -> DatFile {
        let found = install_dats(&auberean_dat::testing::require_target_install()).expect("files");
        let (_, path) = found.iter().find(|(n, _)| n == want).expect("the file");
        DatFile::open(path).expect("opens")
    }

    /// A world's overlay over the install, opened as a start of the server opens it: what it
    /// replaces, adds and deletes is what the files clients ask for hold, in the cell file and the
    /// portal file both, and every other record is the install's.
    /// Behaviour: server.data.answers-from-a-worlds-overlay
    #[test]
    fn the_records_a_client_asks_for_are_read_through_the_worlds_overlay() {
        use dereth_dat::ModernDat;
        use dereth_primitives::DataId;
        use yalain_world::download::DataFiles as _;

        let cell = file_of(&DatName::Cell(1));
        let portal = file_of(&DatName::Portal);
        let blocks: Vec<u32> = cell
            .entries()
            .map(|e| e.id)
            .filter(|id| id & 0xFFFF == 0xFFFF)
            .take(3)
            .collect();
        let (replaced, deleted, kept) = (blocks[0], blocks[1], blocks[2]);
        let in_portal = portal.entries().next().expect("a record").id;
        let world = world_with_overlay("reads", "a-world", |dir| {
            let mut w = dir
                .writer(
                    ModernDat::Cell,
                    cell.shared(),
                    role_file(ModernDat::Cell),
                    "a-world",
                    0,
                )
                .expect("a writer");
            w.put(DataId(replaced), b"the world's own", 1, 0, 0)
                .unwrap();
            w.tombstone(DataId(deleted), 0, 0).unwrap();
            w.flush(0).unwrap();
            let mut w = dir
                .writer(
                    ModernDat::Portal,
                    portal.shared(),
                    role_file(ModernDat::Portal),
                    "a-world",
                    0,
                )
                .expect("a writer");
            w.put(DataId(in_portal), b"the world's too", 1, 0, 0)
                .unwrap();
            w.flush(0).unwrap();
        });
        let overlay = crate::world::overlay_of(&world.display().to_string(), None)
            .expect("reads")
            .expect("a world's overlay");
        let dir = auberean_dat::testing::require_target_install();
        let p = Portal::open(&dir, Some(overlay)).expect("opens");
        let files = install_files(&p);
        let read = |kind, id| files.read(kind, id);
        assert_eq!(
            read(DatKind::Cell, replaced).as_deref(),
            Some(&b"the world's own"[..])
        );
        assert_eq!(read(DatKind::Cell, deleted), None);
        assert_eq!(read(DatKind::Cell, kept), cell.read(kept).ok());
        assert_eq!(
            read(DatKind::Portal, in_portal).as_deref(),
            Some(&b"the world's too"[..])
        );
        assert!(files.overlaid(DatKind::Cell, replaced));
        assert!(files.overlaid(DatKind::Cell, deleted));
        assert!(!files.overlaid(DatKind::Cell, kept));
        std::fs::remove_dir_all(&world).ok();
    }

    /// A world with no overlay reads the install and starts as it did; the pinned release has none.
    /// Behaviour: server.data.answers-from-a-worlds-overlay
    #[test]
    fn a_world_with_no_overlay_is_served_the_installs_own_records() {
        use yalain_world::download::DataFiles as _;

        assert!(crate::world::overlay_of("pinned", None)
            .expect("reads")
            .is_none());
        let world = world_with_overlay("none", "a-world", |_| {});
        let none = crate::world::overlay_of(&world.display().to_string(), None).expect("reads");
        assert!(none.is_none());
        let dir = auberean_dat::testing::require_target_install();
        let p = Portal::open(&dir, none).expect("opens");
        assert!(!p.has_overlay());
        let cell = file_of(&DatName::Cell(1));
        let id = cell.entries().next().expect("a record").id;
        assert_eq!(
            install_files(&p).read(DatKind::Cell, id),
            cell.read(id).ok()
        );
        std::fs::remove_dir_all(&world).ok();
    }

    /// An overlay made against other files stops the start, and so does another world's, whichever
    /// file it lies over: a world is never served from the installed files in place of its own.
    /// Behaviour: server.data.an-overlay-not-for-this-world-stops-the-start
    #[test]
    fn an_overlay_not_for_this_world_and_these_files_stops_the_start() {
        use dereth_dat::ModernDat;
        use dereth_primitives::DataId;

        let cell = file_of(&DatName::Cell(1));
        let portal = file_of(&DatName::Portal);
        let dir = auberean_dat::testing::require_target_install();

        // Made over the portal file, to lie over the cell file.
        let other_files = world_with_overlay("other-files", "a-world", |d| {
            let mut w = d
                .writer(
                    ModernDat::Cell,
                    portal.shared(),
                    role_file(ModernDat::Cell),
                    "a-world",
                    0,
                )
                .expect("a writer");
            w.put(DataId(0x0101_FFFF), b"a record", 1, 0, 0).unwrap();
            w.flush(0).unwrap();
        });
        let overlay = crate::world::overlay_of(&other_files.display().to_string(), None)
            .expect("its world is this one")
            .expect("an overlay");
        let Err(why) = Portal::open(&dir, Some(overlay)) else {
            panic!("an overlay made against other files opened");
        };
        assert!(why.contains("overlay"), "{why}");

        let other_world = world_with_overlay("other-world", "a-world", |d| {
            let mut w = d
                .writer(
                    ModernDat::Cell,
                    cell.shared(),
                    role_file(ModernDat::Cell),
                    "another-world",
                    0,
                )
                .expect("a writer");
            w.put(DataId(0x0101_FFFF), b"a record", 1, 0, 0).unwrap();
            w.flush(0).unwrap();
        });
        let Err(why) = crate::world::overlay_of(&other_world.display().to_string(), None) else {
            panic!("another world's overlay was taken");
        };
        assert!(why.contains("another-world"), "{why}");
        for w in [other_files, other_world] {
            std::fs::remove_dir_all(&w).ok();
        }
    }

    /// A world that writes clearings and has no overlay is told, as it starts.
    /// Behaviour: server.data.a-world-with-clearings-and-no-overlay-is-told
    #[test]
    fn clearings_with_no_overlay_are_warned_about() {
        let files: yalain_world_db::Files = [
            (
                "world.toml".to_owned(),
                "format_version = 1\nkey = \"a\"\ngame = \"ac2\"\nera = \"eor\"\npacks = [\"p\"]\n"
                    .to_owned(),
            ),
            (
                "p/pack.toml".to_owned(),
                "format_version = 1\nname = \"p\"\nversion = \"0.1.0\"\ngame = \"ac2\"\nauthors = []\nlicense = \"CC0-1.0\"\ndescription = \"d\"\n"
                    .to_owned(),
            ),
            (
                "p/server/landblocks/A493.toml".to_owned(),
                "[[clearing]]\nkey = \"c\"\nat = [10.0, 10.0, \"ground\"]\nradius = 5.0\nprov = { from = \"authored\" }\n"
                    .to_owned(),
            ),
        ]
        .into_iter()
        .collect();
        let db = yalain_world_db::rows::read(&files).expect("reads");
        let merged = yalain_world_db::merge::merge(&db).expect("merges");
        assert!(crate::world::clearing_warning(&merged, true).is_none());
        let warned = crate::world::clearing_warning(&merged, false).expect("a warning");
        assert!(warned.contains("1 clearing"), "{warned}");
        let plain = yalain_world_db::merge::Merged::default();
        assert!(crate::world::clearing_warning(&plain, false).is_none());
    }

    /// A data file that counts its reads.
    #[derive(Debug)]
    struct Counted(std::fs::File, Arc<AtomicU64>);

    impl ReadAt for Counted {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
            self.1.fetch_add(1, Ordering::Relaxed);
            self.0.read_at(offset, buf)
        }
    }

    /// The tables that need every saved state read them from the file once between them: the second
    /// to ask reads almost nothing.
    /// Behaviour: server.operator.start-reads-the-saved-states-once
    #[test]
    fn the_mounts_and_the_item_uses_read_the_saved_states_once_between_them() {
        let found = install_dats(&auberean_dat::testing::require_target_install()).expect("files");
        let (_, path) = found
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)
            .expect("a portal");
        let reads = Arc::new(AtomicU64::new(0));
        let file = std::fs::File::open(&path).expect("the file");
        let dat = DatFile::from_storage(
            path.clone(),
            Box::new(Exact(Counted(file, Arc::clone(&reads)))),
        )
        .expect("opens");
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        library(&p).expect("the library");
        let at = |n: &Arc<AtomicU64>| n.load(Ordering::Relaxed);
        let before = at(&reads);
        mounts(&p, &[]).expect("mounts");
        let first = at(&reads) - before;
        let before = at(&reads);
        item_uses(&p, &Catalog::default()).expect("item uses");
        let second = at(&reads) - before;
        assert!(first > 10_000, "the first reads them all: {first}");
        assert!(second < first / 10, "{second} after {first}");
        // The same reading finds the quests' definitions a world's compile asks for.
        let states = saved_states(&p).expect("the states");
        assert!(states.iter().any(|s| s.quest == Some(0x4000_005E)));
        let quests = states.iter().filter(|s| s.quest.is_some()).count();
        assert_eq!(quests, 1741, "only states with phases define quests");
    }

    /// A data file that counts the reads at each offset.
    #[derive(Debug)]
    struct Tallied(std::fs::File, Arc<Mutex<HashMap<u64, u32>>>);

    impl ReadAt for Tallied {
        fn read_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<usize> {
            if let Ok(mut t) = self.1.lock() {
                *t.entry(offset).or_insert(0) += 1;
            }
            self.0.read_at(offset, buf)
        }
    }

    type Tally = Arc<Mutex<HashMap<u64, u32>>>;

    /// An install's file whose reads are tallied, from the time it has opened.
    fn tallied(want: &DatName) -> (DatFile, Tally) {
        let found = install_dats(&auberean_dat::testing::require_target_install()).expect("files");
        let (_, path) = found
            .into_iter()
            .find(|(n, _)| n == want)
            .expect("the file");
        let tally = Tally::default();
        let file = std::fs::File::open(&path).expect("the file");
        let dat = DatFile::from_storage(path, Box::new(Exact(Tallied(file, Arc::clone(&tally)))))
            .expect("opens");
        tally.lock().expect("the tally").clear();
        (dat, tally)
    }

    /// Each entry's reads in `lo..=hi` of a file, at the head of its chain.
    fn heads(dat: &DatFile, tally: &Tally, lo: u32, hi: u32, of: impl Fn(u32) -> bool) -> Vec<u32> {
        let tally = tally.lock().expect("the tally");
        dat.entries_in(lo, hi)
            .filter(|e| of(e.id))
            .filter_map(|e| tally.get(&u64::from(e.offset)).copied())
            .collect()
    }

    /// Reading the ground of two camps side by side reads each landblock and each mesh once, and
    /// reads no material; each mesh's head is read at most a few times besides, not its whole.
    /// Behaviour: server.operator.start-reads-the-ground-once
    #[test]
    fn the_ground_reads_each_landblock_and_mesh_once() {
        let (dat, portal_reads) = tallied(&DatName::Portal);
        let (cell, cell_reads) = tallied(&DatName::Cell(1));
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        p.cell.set(Some(cell)).expect("not yet opened");
        let camp = |cell: u32| yalain_content::Camp {
            id: String::new(),
            cell,
            origin: [0.0; 3],
            radius: 0.0,
            population: 0,
            regen: 0.0,
            templates: Vec::new(),
            spots: Vec::new(),
            placed: String::new(),
            confidence: String::new(),
            moves: yalain_content::movement::CampMoves::default(),
        };
        let spawns = SpawnTable {
            camps: vec![camp(0x75B9_0001), camp(0x76B9_0001)],
        };
        land_ground(&p, &spawns, &Catalog::default()).expect("the ground");
        let cell = p.cell().expect("the cell file");
        let lands = heads(cell, &cell_reads, 0, u32::MAX, |id| id & 0xFFFF == 0xFFFF);
        assert!(
            lands.len() >= 12,
            "the blocks of both camps: {}",
            lands.len()
        );
        assert!(lands.iter().all(|n| *n == 1), "{lands:?}");
        let meshes = heads(&p.dat, &portal_reads, 0x0600_0000, 0x06FF_FFFF, |_| true);
        assert!(!meshes.is_empty(), "the blocks hold objects");
        assert!(meshes.iter().all(|n| *n == 1), "{meshes:?}");
        let materials = heads(&p.dat, &portal_reads, 0x3000_0000, 0x30FF_FFFF, |_| true);
        assert!(materials.is_empty(), "{materials:?}");
    }

    /// The catalog reads what an item's appearance table does to its icon: a recorded armor piece at
    /// its recorded value shows its recorded color.
    /// Behaviour: server.examination.an-items-icon-takes-its-appearance
    #[test]
    fn the_catalog_holds_what_an_items_appearance_table_does_to_its_icon() {
        const ARMOR: u32 = 0x4700_0DC5;
        const TABLE: u32 = 0x2000_0075;
        let (dat, _) = tallied(&DatName::Portal);
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        let (weapon, table, _) = yalain_content::treasure_looks::TREASURE_LOOKS[0];
        let (c, _) = catalog(
            &p,
            &SpawnTable::default(),
            &LootTable::default(),
            &[ARMOR, weapon],
        )
        .expect("the catalog");
        assert!(
            c.icon_tables.contains_key(&table),
            "the table its look draws in"
        );
        let mut icon = c.icons[&ARMOR].clone();
        assert_eq!(icon[1][2], 0xFFFF_FFFF, "its template's");
        yalain_content::apply_icon_value(&mut icon, &c.icon_tables[&TABLE], 2, 0.31);
        assert_eq!(icon[1][2], 0xFF91_1D1D);
        assert!(c
            .icon_tables
            .contains_key(&yalain_world::inventory::POTION_COLOR.0));
    }

    /// Every enchantment treasure may carry, and what it puts on what it hits, reads from the data
    /// files, and what an item shows of it has a name.
    /// Behaviour: server.loot.treasure-enchantments
    #[test]
    fn the_enchantments_treasure_draws_read_from_the_data_files() {
        use yalain_content::treasure_effects::{states, tier_of};
        let (dat, _) = tallied(&DatName::Portal);
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        let wanted: Vec<u32> = states().collect();
        let table = effect_table(&p, &wanted).expect("the table");
        for s in &wanted {
            let d = table.get(*s);
            assert!(d.is_some(), "{s:08x} reads");
            if tier_of(*s).is_some() {
                assert!(d.is_some_and(|d| d.name.is_some()), "{s:08x} has a name");
            }
        }
    }

    /// Every slayer treasure draws reads as one, by the superclass it names and a number of 1.
    /// Behaviour: server.loot.treasure-slayer
    #[test]
    fn the_slayers_treasure_draws_read_as_slayers() {
        use yalain_content::treasure_effects::{states, WEAPON_FAMILIES};
        use yalain_world::effect_defs::Kind;
        const MUCOR: u32 = 0x6F00_03C0;
        let (dat, _) = tallied(&DatName::Portal);
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        let wanted: Vec<u32> = states().collect();
        let table = effect_table(&p, &wanted).expect("the table");
        let family = WEAPON_FAMILIES
            .iter()
            .find(|f| f.tiers.iter().any(|t| t.state == MUCOR))
            .expect("the slayers' family");
        assert_eq!(family.tiers.len(), 31);
        for t in family.tiers {
            let d = table.get(t.state).expect("it reads");
            assert!(
                matches!(&d.kind, Kind::Slayer(c) if !c.is_empty()),
                "{:08x}: {:?}",
                t.state,
                d.kind
            );
        }
        let Some(Kind::Slayer(c)) = table.get(MUCOR).map(|d| &d.kind) else {
            panic!("Mucor's");
        };
        assert_eq!(c, &vec![(25, 1.0)]);
    }

    /// Stasis reads, with the changes its chain holds of other effects' saved states; what it puts on
    /// what it hits comes a tenth of the time.
    /// Behaviour: server.loot.treasure-rarer-enchantments
    #[test]
    fn stasis_reads_with_the_changes_of_its_chain() {
        use yalain_content::treasure_effects::tier_of;
        const STASIS: u32 = 0x6F00_0574;
        const CHAINED: u32 = 0x6F00_2607;
        let (dat, _) = tallied(&DatName::Portal);
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        let table = effect_table(&p, &[STASIS, CHAINED]).expect("the table");
        let (when, tier) = tier_of(STASIS).expect("a tier");
        assert_eq!(when, yalain_content::treasure_effects::When::Hit);
        assert_eq!(tier.puts_on, CHAINED);
        assert!((tier.chance - 0.1).abs() < 1e-3, "{}", tier.chance);
        let d = table.get(CHAINED).expect("its chain reads");
        assert!(d.lasts());
        let changes = d.changes(50.0);
        for stat in [0x3_010B, 0x3_010C, 0x4_0128] {
            assert!(
                changes.iter().any(|(s, _, _)| *s == stat),
                "{stat:x}: {changes:?}"
            );
        }
    }

    /// The tiers named for Water's River and Wounding's Crippling carry the name 2017's items of them
    /// show, not the next tier's.
    /// Behaviour: server.loot.treasure-named-for-an-enchantment
    #[test]
    fn water_and_wounding_tiers_carry_the_names_2017_showed() {
        use yalain_content::treasure_effects::{states, tier_of};
        let (dat, _) = tallied(&DatName::Portal);
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        let wanted: Vec<u32> = states().collect();
        let table = effect_table(&p, &wanted).expect("the table");
        for (state, name) in [(0x6F00_03DB, 0x2500_08C9), (0x6F00_03E0, 0x2500_08CE)] {
            let (_, tier) = tier_of(state).expect("a tier");
            assert_eq!(tier.puts_on, state, "it puts itself on what it hits");
            assert_eq!(
                table.get(state).and_then(|d| d.name).map(|n| n.1),
                Some(name)
            );
        }
    }

    /// The files served to clients are the ones the start read, not opened again; and a record the
    /// tables ask for again is not read again.
    /// Behaviour: server.operator.start-shares-the-open-files
    #[test]
    fn the_files_served_are_the_files_the_start_read_and_a_record_is_read_once() {
        let (dat, reads) = tallied(&DatName::Portal);
        let (cell, _) = tallied(&DatName::Cell(1));
        let (english, _) = tallied(&DatName::Local("English".into()));
        // No file to open: only what the start already holds can be served.
        let p = Portal::over(dat, Vec::new(), None).expect("a context");
        p.cell.set(Some(cell)).expect("not yet opened");
        p.english.set(Some(english)).expect("not yet opened");
        let served = install_files(&p);
        assert!(served.portal.is_some() && served.cell.is_some() && served.english.is_some());
        let total = |r: &Tally| r.lock().expect("the tally").values().sum::<u32>();
        let entity = BODY_ENTITIES[0].2;
        let qualities = p
            .dat
            .entries_in(0x8100_0000, 0x81FF_FFFF)
            .next()
            .expect("qualities")
            .id;
        let visual = p
            .dat
            .entries_in(0x1F00_0000, 0x1FFF_FFFF)
            .next()
            .expect("a look")
            .id;
        assert!(p.entity(entity).is_some());
        assert!(p.qualities(qualities).is_some());
        assert!(p.visual(visual).is_some());
        let before = total(&reads);
        assert!(before >= 3);
        assert!(p.entity(entity).is_some());
        assert!(p.qualities(qualities).is_some());
        assert!(p.visual(visual).is_some());
        assert_eq!(total(&reads), before);
    }
}
