//! A small world of the September 2013 data for tests that play a whole session, written as a
//! world database (`fixtures/hall`) and compiled against the install as the server compiles any:
//! the training hall's copy with a guard (who says a line to a player that uses him), a camp of
//! one sentinel beside the start, its loot, and the hall's exit shard with the town it leads to.
//! The places are authored for the tests, close together so a session reaches each in a few
//! seconds. Beside it, the install's own data files, which a server serves clients' requests for
//! records from ([`install_files`]). (Feature `retail-dats`: it reads the install.)

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use auberean_assets::entity::Entity;
use auberean_assets::script::{Qualities, ScriptState};
use auberean_assets::strings::EnumMap;
use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::DatFile;
use auberean_script::graph::Graph;
use auberean_script::library::{Library, RECORD};
use yalain_content::{describe, facts, Catalog, FactIds, Props, Template, FACT_NAMES};
use yalain_world::levels::{LevelTable, LEVEL_TABLE};
use yalain_world::skills::{SkillTable, SKILL_NAMES};
use yalain_world_db::compile::{compile, Tables};
use yalain_world_db::install::DatInstall;
use yalain_world_db::{check, merge, report, rows, Files};

/// The target install's files a server serves clients' requests for records from: its portal
/// file, its first cell file and its English file.
///
/// # Panics
/// The install is not found.
#[must_use]
pub fn install_files() -> yalain_world::download::InstallFiles {
    let dir = auberean_dat::testing::require_target_install();
    let files = install_dats(&dir).expect("the install's data files");
    let open = |want: &dyn Fn(&DatName) -> bool| {
        files
            .iter()
            .find(|(n, _)| want(n))
            .and_then(|(_, p)| DatFile::open(p).ok())
    };
    yalain_world::download::InstallFiles {
        portal: open(&|n| *n == DatName::Portal),
        cell: open(&|n| *n == DatName::Cell(1)),
        english: open(&|n| matches!(n, DatName::Local(l) if l.eq_ignore_ascii_case("english"))),
    }
}

/// The hall's copy the session plays in (a character starts in it).
pub const HALL: u32 = 0x0298_0109;

/// The shelter's guard, a person.
pub const GUARD: u32 = 0x4700_2DFD;
/// The line the guard says to a player that uses him: (string, table).
pub const GUARD_LINE: (u32, u32) = (0x6369_1D8F, 0x2500_2881);
/// The sentinel, a creature that fights.
pub const SENTINEL: u32 = 0x4700_23B4;
/// Where the sentinels appear.
pub const SENTINEL_SPOT: [f32; 3] = [66.0, 240.0, -44.6];
/// The hall's exit shard, a portal.
pub const SHARD: u32 = 0x4700_1BFF;
pub const SHARD_AT: [f32; 3] = [54.0, 236.0, -44.6];
/// Where the shard takes a player: the town outside.
pub const TOWN: u32 = 0x75B9_0031;
pub const TOWN_AT: [f32; 3] = [131.13, 13.54, 127.26];
/// An item a sentinel leaves.
pub const SWORD: u32 = 0x4700_051C;
/// A corpse (the server makes one with the creature's look; its template describes none).
pub const CORPSE: u32 = 0x4700_0539;
/// The generator-profile names.
const PROFILE_NAMES: u32 = 0x2300_0100;

/// The folder of the world database this module's world is written in.
#[must_use]
pub fn database_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/hall")
}

/// The world database's files.
///
/// # Panics
/// A file does not read.
#[must_use]
pub fn database() -> Files {
    let root = database_dir();
    let mut out = Files::new();
    let mut stack = vec![root.clone()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d)
            .expect("the database's folder")
            .flatten()
        {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let rel = p
                .strip_prefix(&root)
                .expect("inside")
                .to_string_lossy()
                .replace('\\', "/");
            if rel.ends_with(".toml") || rel == "world.lock" {
                out.insert(rel, std::fs::read_to_string(&p).expect("utf-8"));
            }
        }
    }
    out
}

/// The install's portal file and its decoding context.
pub struct Portal {
    pub dat: DatFile,
    pub ctx: Context,
    /// The script library, read once: a catalog asks it of every template.
    library: std::sync::OnceLock<Library>,
    /// The quests' ids by their names, read once.
    quests: std::sync::OnceLock<std::collections::HashMap<String, u32>>,
}

impl std::fmt::Debug for Portal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Portal").finish_non_exhaustive()
    }
}

impl Portal {
    /// The character data's rules.
    ///
    /// # Panics
    /// The data does not read.
    #[must_use]
    pub fn start_rules(&self) -> yalain_world::StartRules {
        let lib = self.library.get_or_init(|| self.library());
        yalain_world::StartRules::from_matrix(lib, &self.matrix()).expect("the rules read")
    }

    /// Which appearance each physique type changes, per race and sex, as the server reads it.
    ///
    /// # Panics
    /// The data does not read.
    #[must_use]
    pub fn physique(
        &self,
    ) -> std::collections::BTreeMap<(u32, u32), Vec<yalain_world::chargen::PhysiqueEffect>> {
        let lib = self.library.get_or_init(|| self.library());
        yalain_world::chargen::physique_of_matrix(lib, &self.matrix()).expect("the map reads")
    }

    /// Each race and sex's body entity, as the server reads them from the character generation
    /// data.
    ///
    /// # Panics
    /// The data does not read.
    #[must_use]
    pub fn body_entities(&self) -> std::collections::BTreeMap<(u32, u32), u32> {
        let lib = self.library.get_or_init(|| self.library());
        yalain_world::chargen::body_entities_of_matrix(lib, &self.matrix())
            .expect("the table reads")
    }

    /// The character generation data's graph.
    fn matrix(&self) -> auberean_script::graph::Graph {
        let lib = self.library.get_or_init(|| self.library());
        let bytes = self
            .dat
            .read(0x7000_0390)
            .expect("character generation data");
        let st: auberean_assets::script::ScriptState =
            auberean_assets::decode(0x7000_0390, &bytes, &self.ctx).expect("reads");
        auberean_script::graph::Graph::decode(&st.graph, lib).expect("its graph reads")
    }

    /// The script library.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn library(&self) -> Library {
        self.dat
            .read_checked(RECORD, Library::from_record)
            .expect("the library reads")
            .value
    }

    /// The target install's portal file.
    ///
    /// # Panics
    /// The install is not found, or its portal file does not read.
    #[must_use]
    pub fn open() -> Self {
        let dir = auberean_dat::testing::require_target_install();
        let files = install_dats(&dir).expect("the install's data files");
        let (_, p) = files
            .into_iter()
            .find(|(n, _)| *n == DatName::Portal)
            .expect("a portal file");
        let dat = DatFile::open(&p).expect("the portal file opens");
        let ctx = Context::from_portal(&dat).expect("its context reads");
        Self {
            dat,
            ctx,
            library: std::sync::OnceLock::new(),
            quests: std::sync::OnceLock::new(),
        }
    }

    fn entity(&self, id: u32) -> Option<Entity> {
        auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()
    }

    fn qualities(&self, id: u32) -> Option<Qualities> {
        auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()
    }

    fn fact_ids(&self) -> FactIds {
        let id = |name: &str| {
            self.ctx
                .properties()
                .and_then(|l| l.names.names.iter().find(|(_, n)| n == name))
                .map_or(0, |(v, _)| *v)
        };
        let n = FACT_NAMES;
        FactIds {
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

    /// The quests' ids by their names, as the install's enumeration of them gives them.
    ///
    /// # Panics
    /// The enumeration does not read.
    #[must_use]
    pub fn quest_ids(&self) -> &std::collections::HashMap<String, u32> {
        self.quests.get_or_init(|| {
            let id = yalain_world_db::install::QUEST_NAMES;
            let names: EnumMap = auberean_assets::decode(
                id,
                &self.dat.read(id).expect("the quests' names"),
                &self.ctx,
            )
            .expect("the quests' names read");
            names.names.into_iter().map(|(v, n)| (n, v)).collect()
        })
    }

    /// A template of the install, described.
    ///
    /// # Panics
    /// The template does not describe.
    #[must_use]
    pub fn template(&self, id: u32) -> Template {
        self.described(id).expect("the template is described")
    }

    /// A template of the install, described: none when it does not describe.
    #[must_use]
    pub fn described(&self, id: u32) -> Option<Template> {
        let name = |p: u32| {
            self.ctx
                .properties()
                .and_then(|l| l.names.name(p))
                .map(str::to_string)
        };
        let class = Props::class_of(id, &|e| self.entity(e)).map_or_else(Vec::new, |c| {
            yalain_world_db::install::class_properties(
                self.library.get_or_init(|| self.library()),
                c,
            )
        });
        let props = Props::of(id, &|e| self.entity(e), &name)
            .with_places_of(self.qualities(yalain_content::qualities_id(id)).as_ref())
            .with_worn_of(id)
            .with_class(&class)
            .with_quest_named(&|n| self.quest_ids().get(n).copied());
        Some(Template {
            id,
            description: describe(id, &|e| self.entity(e), &|q| self.qualities(q)).ok()?,
            facts: facts(id, &|e| self.entity(e), &self.fact_ids()).named_by(&props),
            props,
        })
    }

    /// The properties of every template of the install, by template, as a template's own and its
    /// parents' entities give them.
    #[must_use]
    pub fn every_template_props(&self) -> Vec<(u32, Props)> {
        let name = |p: u32| {
            self.ctx
                .properties()
                .and_then(|l| l.names.name(p))
                .map(str::to_string)
        };
        let ids: Vec<u32> = self
            .dat
            .entries_in(0x4700_0000, 0x47FF_FFFF)
            .map(|e| e.id)
            .collect();
        ids.into_iter()
            .map(|id| (id, Props::of(id, &|e| self.entity(e), &name)))
            .collect()
    }

    /// The skills' definitions and advancement tables, as the server reads them at start.
    ///
    /// # Panics
    /// The library or the skills' names do not read.
    #[must_use]
    pub fn skills(&self) -> SkillTable {
        let lib = self.library();
        let names: EnumMap = auberean_assets::decode(
            SKILL_NAMES,
            &self.dat.read(SKILL_NAMES).expect("the skills' names"),
            &self.ctx,
        )
        .expect("the skills' names read");
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        SkillTable::read(&lib, &names.names, &graph)
    }

    /// The skills each kingdom's shrine grants by points, as the server reads them at start.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn kingdom_skills(&self) -> yalain_world::kingdom_skills::KingdomSkills {
        let skills = self.skills();
        let lib = self.library();
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        yalain_world::kingdom_skills::KingdomSkills::read(&lib, &graph, &skills)
    }

    /// The effects of these saved states (and those they add), as the server reads them at start
    /// with a world that has no `[[effect_rule]]` rows.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn effects(&self, states: &[u32]) -> yalain_world::effect_defs::EffectTable {
        self.effects_warded(states, &yalain_world::effect_defs::fallback_wards())
    }

    /// The effects of these saved states (and those they add), as the server reads them at start
    /// with these wards (a world's, [`yalain_world::effect_defs::world_wards`]).
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn effects_warded(
        &self,
        states: &[u32],
        wards: &std::collections::BTreeMap<u32, yalain_world::effect_defs::Ward>,
    ) -> yalain_world::effect_defs::EffectTable {
        let lib = self.library();
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        yalain_world::effect_defs::EffectTable::read(&lib, states, &graph, wards)
    }

    /// The mounts as the server reads them at start: the mount effects among the saved states,
    /// the mount table's looks, the bodies' own appearance, and which of these templates mount
    /// their users.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn mounts(&self, templates: &[u32]) -> yalain_world::mounts::Mounts {
        let lib = self.library();
        let states: Vec<(u32, u32)> = self
            .dat
            .entries_in(0x6F00_0000, 0x6FFF_FFFF)
            .map(|e| e.id)
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|id| {
                let st: ScriptState =
                    auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
                Some((id, st.package))
            })
            .collect();
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        let bodies: Vec<u32> = self
            .body_entities()
            .values()
            .filter_map(|e| self.entity(*e).and_then(|e| e.object))
            .collect();
        let mut m = yalain_world::mounts::Mounts::read(
            &lib,
            &states,
            &graph,
            &|id| self.dat.read(id).ok(),
            &bodies,
        );
        m.note_items(&self.item_uses(templates));
        m
    }

    /// The install's templates whose class is this script package or derives from it, in order.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn templates_of_class(&self, class: &str) -> Vec<u32> {
        let lib = self.library();
        let Some(base) = lib.package_id(class) else {
            return Vec::new();
        };
        let ids: Vec<u32> = self
            .dat
            .entries_in(0x4700_0000, 0x47FF_FFFF)
            .map(|e| e.id)
            .collect();
        ids.into_iter()
            .filter(|id| {
                Props::class_of(*id, &|e| self.entity(e)).is_some_and(|c| lib.derives_from(c, base))
            })
            .collect()
    }

    /// The mount items the world's catalog always holds, as the server reads them at start: the
    /// saddle class's templates whose use mounts their user, and the eggs.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn mount_items(&self) -> Vec<u32> {
        let lib = self.library();
        let Some(saddle) = lib.package_id(yalain_world::mounts::SADDLE_CLASS) else {
            return yalain_world::mounts::EGGS.to_vec();
        };
        let ids: Vec<u32> = self
            .dat
            .entries_in(0x4700_0000, 0x47FF_FFFF)
            .map(|e| e.id)
            .collect();
        let saddles: Vec<u32> = ids
            .into_iter()
            .filter(|id| {
                self.entity(*id).is_some_and(|e| {
                    e.kind == Some(0x4000_0002)
                        && e.object.is_some_and(|pkg| lib.derives_from(pkg, saddle))
                })
            })
            .collect();
        let mut m = self.mounts(&[]);
        m.note_items(&self.item_uses(&saddles));
        m.catalog_items()
    }

    /// Does a template's script package derive from any of the script library's packages named?
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn derives(&self, template: u32, names: &[&str]) -> bool {
        let lib = self.library();
        let package = self.template(template).description.package;
        names
            .iter()
            .filter_map(|n| lib.package_id(n))
            .any(|b| lib.derives_from(package, b))
    }

    /// The interaction tables these item templates name, as the server reads them, with these
    /// templates (each with its script package) as the objects a class they name may count.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn interactions(
        &self,
        templates: &[u32],
        targets: &[(u32, u32)],
    ) -> yalain_world::usable::interactions::Interactions {
        let lib = self.library();
        let states: Vec<u32> = templates
            .iter()
            .filter_map(|t| {
                self.template(*t)
                    .props
                    .word(yalain_world::usable::interactions::TABLE)
            })
            .collect();
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        yalain_world::usable::interactions::Interactions::read(&lib, &states, &graph, targets)
    }

    /// The effects these templates' uses put on, as the server reads them at start.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn item_uses(&self, templates: &[u32]) -> yalain_world::item_use::ItemUses {
        self.named_effects(templates, yalain_world::item_use::USED, &HashMap::new())
    }

    /// The effects these templates' uses put on, as the server reads them at start with a world
    /// that gives these effects' names their saved states.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn item_uses_named(
        &self,
        templates: &[u32],
        named: &HashMap<String, u32>,
    ) -> yalain_world::item_use::ItemUses {
        self.named_effects(templates, yalain_world::item_use::USED, named)
    }

    /// The effects these templates put on whoever wears or wields them, as the server reads them
    /// at start.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn worn_effects(&self, templates: &[u32]) -> yalain_world::item_use::ItemUses {
        self.named_effects(templates, yalain_world::item_use::WORN, &HashMap::new())
    }

    /// The effects these templates' deaths name, as the server reads them at start.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn death_effects(&self, templates: &[u32]) -> yalain_world::item_use::ItemUses {
        self.named_effects(templates, yalain_world::item_use::DIED, &HashMap::new())
    }

    fn named_effects(
        &self,
        templates: &[u32],
        props: (&str, &str),
        named: &HashMap<String, u32>,
    ) -> yalain_world::item_use::ItemUses {
        let lib = self.library();
        let ids: Vec<u32> = self
            .dat
            .entries_in(0x6F00_0000, 0x6FFF_FFFF)
            .map(|e| e.id)
            .collect();
        let states = ids.into_iter().filter_map(|id| {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            let value = Graph::decode(&st.graph, &lib)
                .ok()
                .and_then(|g| yalain_world::effect_defs::EffectDef::enum_value_of(&lib, &g));
            Some((id, st.package, value))
        });
        let described: Vec<(u32, Template)> = templates
            .iter()
            .map(|id| (*id, self.template(*id)))
            .collect();
        yalain_world::item_use::read_named_or(
            &lib,
            states,
            described.iter().map(|(id, t)| (*id, &t.props)),
            props,
            named,
        )
    }

    /// The craft skills and recipes of these saved states, as the server reads them at start.
    ///
    /// # Panics
    /// The library does not read.
    #[must_use]
    pub fn crafting(&self, skills: &[u32], recipes: &[u32]) -> yalain_world::craft::Crafting {
        let lib = self.library();
        let graph = |id: u32| -> Option<Graph> {
            let st: ScriptState =
                auberean_assets::decode(id, &self.dat.read(id).ok()?, &self.ctx).ok()?;
            Graph::decode(&st.graph, &lib).ok()
        };
        yalain_world::craft::Crafting::read(&lib, skills, recipes, &[], &graph)
    }

    /// The body a template's look gives (its radius and height), as the server reads it at start.
    #[must_use]
    pub fn body(&self, template: u32) -> Option<[f32; 2]> {
        let look = self.template(template).description.look;
        let visual =
            |did: u32| auberean_assets::decode(did, &self.dat.read(did).ok()?, &self.ctx).ok();
        let setup = |did: u32| {
            let s: auberean_assets::setup::Setup =
                auberean_assets::decode(did, &self.dat.read(did).ok()?, &self.ctx).ok()?;
            Some((s.radius, s.height))
        };
        yalain_content::look_body(look, &visual, &setup)
    }

    /// The level table.
    ///
    /// # Panics
    /// The table does not read.
    #[must_use]
    pub fn levels(&self) -> LevelTable {
        let lib = self.library();
        let st: ScriptState = auberean_assets::decode(
            LEVEL_TABLE,
            &self.dat.read(LEVEL_TABLE).expect("the level table"),
            &self.ctx,
        )
        .expect("the level table reads");
        let g = Graph::decode(&st.graph, &lib).expect("its graph reads");
        LevelTable::from_state(&lib, &g).expect("the levels read")
    }

    /// The game's Arcane Lore table, as the server reads it.
    ///
    /// # Panics
    /// The table does not read.
    #[must_use]
    pub fn lore_table(&self) -> yalain_world::craft::LevelMap {
        use yalain_world::treasure::{lore_table, ARCANE_LORE_TABLE};
        let lib = self.library.get_or_init(|| self.library());
        let st: ScriptState = auberean_assets::decode(
            ARCANE_LORE_TABLE,
            &self
                .dat
                .read(ARCANE_LORE_TABLE)
                .expect("the Arcane Lore table"),
            &self.ctx,
        )
        .expect("the Arcane Lore table reads");
        let g = Graph::decode(&st.graph, lib).expect("its graph reads");
        lore_table(lib, &g).expect("the lore reads")
    }

    /// A world database's files checked, merged and compiled against this install.
    ///
    /// # Panics
    /// The database has a problem, or a row names what the install lacks.
    #[must_use]
    pub fn compile(&self, files: &Files) -> Tables {
        let problems = check::check(files);
        assert!(problems.is_empty(), "{}", report(&problems));
        let db = rows::read(files).unwrap_or_else(|p| panic!("{}", report(&p)));
        let merged = merge::merge(&db).unwrap_or_else(|p| panic!("{}", report(&p)));
        let states = self
            .dat
            .entries_in(0x6F00_0000, 0x6FFF_FFFF)
            .map(|e| e.id)
            .collect();
        let lib = self.library();
        let install = DatInstall::new(
            Box::new(|id| self.dat.read(id).ok()),
            self.ctx.clone(),
            &lib,
            states,
        )
        .expect("the install reads");
        compile(&merged, &install).unwrap_or_else(|p| panic!("{}", report(&p)))
    }

    /// The templates a compiled world makes things of (every template a camp spawns, every item
    /// its loot names, and the corpse), and the generator profiles' names.
    ///
    /// # Panics
    /// The profile names or a template do not read.
    #[must_use]
    pub fn catalog(&self, t: &Tables) -> Catalog {
        let mut catalog = Catalog::default();
        let names: EnumMap = auberean_assets::decode(
            PROFILE_NAMES,
            &self.dat.read(PROFILE_NAMES).expect("the profile names"),
            &self.ctx,
        )
        .expect("the profile names read");
        catalog.profiles = names.names.into_iter().collect();
        let spawned = t
            .spawns
            .camps
            .iter()
            .flat_map(|c| c.templates.iter().map(|(t, _)| *t));
        let dropped = t
            .loot
            .profiles
            .iter()
            .flat_map(|p| p.items.iter().map(|(t, _)| *t));
        // Everything the server's rules and the world's rows make, as a real start asks for it.
        let made = yalain_world::makes::by_the_server(&self.start_rules(), &self.mount_items())
            .into_iter()
            .chain(yalain_world::makes::by_the_rows(
                yalain_world::makes::Rows {
                    objects: &t.content.objects,
                    stores: &t.stores,
                    quests: &t.quests,
                    crafting: &t.crafting,
                    death_spawns: &t.death_spawns,
                    pets: &t.pet_kinds,
                },
            ));
        // The looks treasure draws: 2017's, as a world without its own plays by.
        let treasure = yalain_content::treasure_tables::TreasureTables::retail_2017();
        let mut seen = std::collections::HashSet::new();
        for id in spawned
            .chain(dropped)
            .chain(made)
            .filter(|id| seen.insert(*id))
        {
            let t = self.template(id);
            let icon = yalain_content::icon_layers(t.description.look, &|did| {
                auberean_assets::decode(did, &self.dat.read(did).ok()?, &self.ctx).ok()
            });
            if !icon.is_empty() {
                catalog.icons.insert(id, icon);
            }
            let tables = yalain_world::equip::dye_table(&t.props)
                .into_iter()
                .chain(treasure.look_table(id));
            for table in tables {
                catalog.icon_tables.entry(table).or_insert_with(|| {
                    yalain_content::icon_tables(table, &|did| {
                        auberean_assets::decode(did, &self.dat.read(did).ok()?, &self.ctx).ok()
                    })
                });
            }
            catalog.insert(t);
        }
        catalog
    }
}

/// The authored world, compiled against the install.
///
/// # Panics
/// As [`Portal::compile`].
#[must_use]
pub fn world(p: &Portal) -> Tables {
    p.compile(&database())
}
