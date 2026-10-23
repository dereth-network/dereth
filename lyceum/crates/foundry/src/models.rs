//! What a model is: the weenies drawn with it, the setups it is a part of, the terrains it grows
//! on as scenery, and where the world places it (outdoors, inside, as a building, and in which
//! town). From those, the category the world editor's model browser files it under and the few
//! words it shows beside it. The data files name no model.
//!
//! The tags a user gives models are kept beside the world, in [`Tags`].

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_assets::geometry::{GfxObj, Setup};
use dereth_assets::material::Surface;
use dereth_assets::world::{EnvCell, LandblockInfo, Scene};
use dereth_assets::Region;
use dereth_primitives::DataId;

use crate::dats::DatSet;
use crate::pick::Models;
use crate::sql_ids::WeenieFacts;

/// What kind of thing a model draws, as the browser files it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    Npc,
    Creature,
    Door,
    Portal,
    Chest,
    Lifestone,
    /// A thing fixed in the world that a weenie draws: a sign, a statue, a switch.
    Object,
    /// A thing that can be carried.
    Item,
    Tree,
    Rock,
    Plant,
    Building,
    /// A piece of a town placed outdoors: a wall, a fence, a well, a lamp.
    BuildingPiece,
    /// A thing placed inside a building or a dungeon.
    Furniture,
    /// A model only ever drawn as a part of another.
    Part,
    Other,
}

impl Category {
    /// Every category, in the browser's order.
    pub const ALL: [Self; 16] = [
        Self::Npc,
        Self::Creature,
        Self::Door,
        Self::Portal,
        Self::Chest,
        Self::Lifestone,
        Self::Object,
        Self::Item,
        Self::Tree,
        Self::Rock,
        Self::Plant,
        Self::Building,
        Self::BuildingPiece,
        Self::Furniture,
        Self::Part,
        Self::Other,
    ];

    /// What the browser calls it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Npc => "NPC",
            Self::Creature => "Creature",
            Self::Door => "Door",
            Self::Portal => "Portal",
            Self::Chest => "Chest",
            Self::Lifestone => "Lifestone",
            Self::Object => "Object",
            Self::Item => "Item",
            Self::Tree => "Tree",
            Self::Rock => "Rock",
            Self::Plant => "Plant",
            Self::Building => "Building",
            Self::BuildingPiece => "Building piece",
            Self::Furniture => "Furniture",
            Self::Part => "Part",
            Self::Other => "Other",
        }
    }

    /// The category a weenie is in, by its SQL: its weenie type, and for a creature whether it
    /// can be attacked, for anything else whether it is stuck in place.
    #[must_use]
    pub fn of_weenie(f: &WeenieFacts) -> Self {
        match f.kind {
            // Creature and Cow.
            Some(10 | 15) if f.attackable == Some(false) => Self::Npc,
            Some(10 | 15) => Self::Creature,
            // Vendor.
            Some(12) => Self::Npc,
            Some(19) => Self::Door,
            // Portal and HousePortal.
            Some(7 | 59) => Self::Portal,
            Some(20) => Self::Chest,
            Some(25) => Self::Lifestone,
            // HotSpot, PressurePlate, Switch, LightSource, House, SlumLord, Hook, Storage,
            // BootSpot, AllegianceBindstone: fixed where they stand.
            Some(13 | 24 | 26 | 29 | 53 | 55 | 56 | 57 | 58 | 65) => Self::Object,
            // Undef and Generic: a sign or a statue is stuck in place.
            None | Some(0 | 1) if f.stuck == Some(true) => Self::Object,
            _ => Self::Item,
        }
    }
}

/// How the world uses one model.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelUse {
    /// The weenies drawn with it, by name, the name most of them have first.
    pub weenies: Vec<String>,
    /// How many weenies are drawn with it, and the category most of them are in.
    pub weenie_count: u32,
    pub weenie_category: Option<Category>,
    /// The terrain types whose scenery it is, by name, in the region's order.
    pub scenery: Vec<String>,
    /// Whether the scenery is tilted to the ground's slope where it grows, as rocks are.
    pub aligned: bool,
    /// Whether some surface of the scenery is cut out of its picture, as leaves are.
    pub cut_out: bool,
    /// The scenery's width and height, in meters.
    pub size: Option<[f32; 2]>,
    /// How many landblocks place it, outdoors, inside or as a building.
    pub landblocks: u32,
    /// How many times the world places it as a building, as an outdoor static and inside a
    /// cell.
    pub buildings: u32,
    pub outdoors: u32,
    pub inside: u32,
    /// The towns whose landblocks place it, the one that places it most first.
    pub towns: Vec<String>,
    /// The setups it is a part of.
    pub parents: Vec<u32>,
}

/// Every model's use, and from it each one's category and label.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelNames {
    uses: BTreeMap<u32, ModelUse>,
    categories: BTreeMap<u32, Category>,
    labels: BTreeMap<u32, String>,
    /// Each label in lower case, and each model's words a filter can find it by (as
    /// [`Self::matches`] reads them), in lower case, one to a line.
    lower_labels: BTreeMap<u32, String>,
    words: BTreeMap<u32, String>,
}

/// What the data files say of every model: where the world places it, the scenery it is and
/// the setups it is a part of. Reading it takes a few seconds, so it is kept in a file
/// ([`Placements::save`]) for the next time.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Placements {
    uses: BTreeMap<u32, ModelUse>,
}

/// The first line of a placements file, which names its layout.
const PLACEMENTS_HEADER: &str = "# model placements 1";

/// A terrain type's name as the region writes it (`LushGrass`, `sand-yellow`) in words
/// (`Lush grass`, `Sand-yellow`).
#[must_use]
pub fn words(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if i == 0 {
            out.push(c.to_ascii_uppercase());
        } else if c.is_ascii_uppercase() && !out.ends_with(' ') {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// Scenery's category by its shape: nothing to see is other; tilted to the slope where it grows
/// is a rock; with leaves (surfaces cut out of their pictures) a tree when tall, else a plant;
/// small or slender is a plant, and the rest a rock.
fn shape_category(size: Option<[f32; 2]>, aligned: bool, cut_out: bool) -> Category {
    let Some([w, h]) = size else {
        return Category::Other;
    };
    let tall = h >= 3.0 && h >= 0.6 * w;
    if w < 0.1 {
        Category::Other
    } else if aligned {
        Category::Rock
    } else if cut_out {
        if tall {
            Category::Tree
        } else {
            Category::Plant
        }
    } else if (h < 1.6 && w < 3.0) || (w < 1.2 && h < 3.0) {
        Category::Plant
    } else {
        Category::Rock
    }
}

/// Whether some surface of model `id` (a GfxObj, or a Setup's parts) is cut out of its picture:
/// drawn only where the picture is solid, as leaves are.
fn cut_out(set: &DatSet, id: DataId) -> bool {
    let parts = match id.raw() >> 24 {
        0x02 => set
            .read_portal(id)
            .and_then(|b| set.decode::<Setup>(id, &b).ok())
            .map(|s| s.parts)
            .unwrap_or_default(),
        _ => vec![id],
    };
    parts.iter().any(|p| {
        set.read_portal(*p)
            .and_then(|b| set.decode::<GfxObj>(*p, &b).ok())
            .is_some_and(|g| {
                g.surfaces.iter().any(|s| {
                    set.read_portal(*s)
                        .and_then(|b| set.decode::<Surface>(*s, &b).ok())
                        .is_some_and(|s| s.surface_type & 4 != 0)
                })
            })
    })
}

impl Placements {
    /// What `set` says: its statics records and its cells, its setups' parts, and the scenery
    /// of `region`, with the town `town` names a landblock as being in.
    #[must_use]
    pub fn read(
        set: &DatSet,
        region: Option<&Region>,
        town: &dyn Fn(u16) -> Option<String>,
    ) -> Self {
        let mut uses: BTreeMap<u32, ModelUse> = BTreeMap::new();
        if let Some(region) = region {
            // Each model's terrains, in the region's order of them.
            let mut scenery: BTreeMap<u32, Vec<String>> = BTreeMap::new();
            let mut aligned: BTreeSet<u32> = BTreeSet::new();
            let scene_types = region.scene_info.as_deref().unwrap_or_default();
            let mut scenes: BTreeMap<u32, Vec<(u32, bool)>> = BTreeMap::new();
            for t in &region.terrain_types {
                for st in &t.scene_types {
                    let Some(desc) = usize::try_from(*st).ok().and_then(|i| scene_types.get(i))
                    else {
                        continue;
                    };
                    for scene in &desc.scenes {
                        let models = scenes.entry(scene.raw()).or_insert_with(|| {
                            set.read_portal(*scene)
                                .and_then(|b| set.decode::<Scene>(*scene, &b).ok())
                                .map(|s| {
                                    s.objects
                                        .iter()
                                        .map(|o| (o.obj_id.raw(), o.align != 0))
                                        .collect()
                                })
                                .unwrap_or_default()
                        });
                        for (m, a) in models.iter() {
                            let list = scenery.entry(*m).or_default();
                            let name = words(&t.terrain_name);
                            if !list.contains(&name) {
                                list.push(name);
                            }
                            if *a {
                                aligned.insert(*m);
                            }
                        }
                    }
                }
            }
            let mut shapes = Models::default();
            for (m, names) in scenery {
                let u = uses.entry(m).or_default();
                u.scenery = names;
                u.aligned = aligned.contains(&m);
                u.cut_out = cut_out(set, DataId(m));
                u.size = shapes.get(set, DataId(m)).and_then(|model| {
                    let mut lo = [f32::INFINITY; 3];
                    let mut hi = [f32::NEG_INFINITY; 3];
                    for p in model.triangles.iter().flatten() {
                        for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                            lo[i] = lo[i].min(v);
                            hi[i] = hi[i].max(v);
                        }
                    }
                    (lo[0] <= hi[0]).then(|| [(hi[0] - lo[0]).max(hi[1] - lo[1]), hi[2] - lo[2]])
                });
            }
        }
        // Where the world places each model, and in which towns.
        let mut towns: BTreeMap<u16, Option<String>> = BTreeMap::new();
        let mut in_town: BTreeMap<u32, BTreeMap<String, u32>> = BTreeMap::new();
        let mut blocks: BTreeMap<u32, BTreeSet<u16>> = BTreeMap::new();
        let mut place = |m: u32, lb: u16, how: u8, uses: &mut BTreeMap<u32, ModelUse>| {
            let u = uses.entry(m).or_default();
            match how {
                0 => u.buildings += 1,
                1 => u.outdoors += 1,
                _ => u.inside += 1,
            }
            blocks.entry(m).or_default().insert(lb);
            if let Some(t) = towns.entry(lb).or_insert_with(|| town(lb)) {
                *in_town.entry(m).or_default().entry(t.clone()).or_default() += 1;
            }
        };
        for id in set.cell.iter_ids() {
            #[allow(clippy::cast_possible_truncation)] // LINT-OK: the high half is the landblock.
            let lb = (id.raw() >> 16) as u16;
            match id.raw() & 0xFFFF {
                0xFFFE => {
                    let Some(info) = set
                        .cell
                        .read(id)
                        .ok()
                        .and_then(|b| set.decode::<LandblockInfo>(id, &b).ok())
                    else {
                        continue;
                    };
                    for b in &info.buildings {
                        place(b.id.raw(), lb, 0, &mut uses);
                    }
                    for o in &info.objects {
                        place(o.id.raw(), lb, 1, &mut uses);
                    }
                }
                0x0100..0xFFFE => {
                    let Some(cell) = set
                        .cell
                        .read(id)
                        .ok()
                        .and_then(|b| set.decode::<EnvCell>(id, &b).ok())
                    else {
                        continue;
                    };
                    for o in &cell.static_objects {
                        place(o.id.raw(), lb, 2, &mut uses);
                    }
                }
                _ => {}
            }
        }
        for (m, b) in blocks {
            if let Some(u) = uses.get_mut(&m) {
                u.landblocks = u32::try_from(b.len()).unwrap_or(u32::MAX);
            }
        }
        for (m, t) in in_town {
            let mut t: Vec<(String, u32)> = t.into_iter().collect();
            t.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            if let Some(u) = uses.get_mut(&m) {
                u.towns = t.into_iter().map(|(n, _)| n).collect();
            }
        }
        // The setups each model is a part of.
        for id in set.portal.iter_ids() {
            if id.raw() >> 24 != 0x02 {
                continue;
            }
            let Some(s) = set
                .read_portal(id)
                .and_then(|b| set.decode::<Setup>(id, &b).ok())
            else {
                continue;
            };
            let parts: BTreeSet<u32> = s.parts.iter().map(|p| p.raw()).collect();
            for p in parts {
                if p != id.raw() {
                    uses.entry(p).or_default().parents.push(id.raw());
                }
            }
        }
        Self { uses }
    }

    /// The placements as a file's text: a header line, then a line a model, its fields
    /// separated by tabs and its lists by `|` (the flags are whether it is aligned and whether it
    /// is cut out, `0` or `1` each).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::from(PLACEMENTS_HEADER);
        out.push('\n');
        for (m, u) in &self.uses {
            let size = u
                .size
                .map_or_else(|| "\t".to_owned(), |[w, h]| format!("{w}\t{h}"));
            let parents: Vec<String> = u.parents.iter().map(|p| format!("{p:08X}")).collect();
            out.push_str(&format!(
                "{m:08X}\t{}{}\t{size}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                u8::from(u.aligned),
                u8::from(u.cut_out),
                u.landblocks,
                u.buildings,
                u.outdoors,
                u.inside,
                u.scenery.join("|"),
                u.towns.join("|"),
                parents.join("|"),
            ));
        }
        out
    }

    /// The placements [`Self::to_text`] wrote; `None` for any other text.
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        let mut lines = text.lines();
        if lines.next()? != PLACEMENTS_HEADER {
            return None;
        }
        let list = |s: &str| -> Vec<String> {
            if s.is_empty() {
                Vec::new()
            } else {
                s.split('|').map(str::to_owned).collect()
            }
        };
        let mut uses = BTreeMap::new();
        for line in lines {
            let f: Vec<&str> = line.split('\t').collect();
            let [m, flags, w, h, landblocks, buildings, outdoors, inside, scenery, towns, parents] =
                f[..]
            else {
                return None;
            };
            let size = match (w.parse::<f32>(), h.parse::<f32>()) {
                (Ok(w), Ok(h)) => Some([w, h]),
                _ => None,
            };
            let parents = list(parents)
                .iter()
                .map(|p| u32::from_str_radix(p, 16).ok())
                .collect::<Option<Vec<u32>>>()?;
            uses.insert(
                u32::from_str_radix(m, 16).ok()?,
                ModelUse {
                    aligned: flags.starts_with('1'),
                    cut_out: flags.ends_with('1') && flags.len() == 2,
                    size,
                    landblocks: landblocks.parse().ok()?,
                    buildings: buildings.parse().ok()?,
                    outdoors: outdoors.parse().ok()?,
                    inside: inside.parse().ok()?,
                    scenery: list(scenery),
                    towns: list(towns),
                    parents,
                    ..ModelUse::default()
                },
            );
        }
        Some(Self { uses })
    }

    /// Keep the placements in the file `path`.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let part = path.with_extension("part");
        std::fs::write(&part, self.to_text())?;
        std::fs::rename(&part, path)
    }

    /// The placements kept in the file `path`, when it holds them.
    #[must_use]
    pub fn load(path: &Path) -> Option<Self> {
        Self::from_text(&std::fs::read_to_string(path).ok()?)
    }
}

/// What each scene type of terrain type `terrain` grows, by the region's scene table: for each
/// scene value a vertex can hold, the models its scenes place, each once, in the scenes' order.
/// A value past the terrain's list grows nothing and is not listed.
#[must_use]
pub fn scenery_of(set: &DatSet, region: &Region, terrain: u16) -> Vec<Vec<DataId>> {
    let Some(t) = region.terrain_types.get(usize::from(terrain)) else {
        return Vec::new();
    };
    let descs = region.scene_info.as_deref().unwrap_or_default();
    t.scene_types
        .iter()
        .map(|st| {
            let mut models: Vec<DataId> = Vec::new();
            let scenes = usize::try_from(*st)
                .ok()
                .and_then(|i| descs.get(i))
                .map(|d| d.scenes.as_slice())
                .unwrap_or_default();
            for scene in scenes {
                let objects = set
                    .read_portal(*scene)
                    .and_then(|b| set.decode::<Scene>(*scene, &b).ok())
                    .map(|s| s.objects)
                    .unwrap_or_default();
                for o in objects {
                    if o.obj_id.raw() != 0 && !models.contains(&o.obj_id) {
                        models.push(o.obj_id);
                    }
                }
            }
            models
        })
        .collect()
}

/// The word most of `names` share (`Drudge` of `Drudge Prowler`, `Ascendant Drudge`, ...), when
/// at least three and at least half of them do.
fn common_word(names: &[String]) -> Option<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in names {
        let words: BTreeSet<String> = n
            .split(|c: char| !c.is_alphanumeric() && c != '\'')
            .filter(|w| w.len() > 2 && w.chars().next().is_some_and(char::is_uppercase))
            .filter(|w| !matches!(*w, "The" | "And" | "For"))
            .map(str::to_owned)
            .collect();
        for w in words {
            *counts.entry(w).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, c)| *c >= 3 && *c * 2 >= names.len())
        .max_by_key(|(w, c)| (*c, std::cmp::Reverse(w.clone())))
        .map(|(w, _)| w)
}

impl ModelNames {
    /// The placements `set` holds (as [`Placements::read`]) and the weenies `weenies` gives each
    /// setup (as [`crate::edit::Session::weenies_by_setup`]).
    #[must_use]
    pub fn read(
        set: &DatSet,
        region: Option<&Region>,
        weenies: &BTreeMap<u32, Vec<WeenieFacts>>,
        town: &dyn Fn(u16) -> Option<String>,
    ) -> Self {
        Self::new(&Placements::read(set, region, town), weenies)
    }

    /// The models `placements` places, with the weenies `weenies` gives each setup.
    #[must_use]
    pub fn new(placements: &Placements, weenies: &BTreeMap<u32, Vec<WeenieFacts>>) -> Self {
        let mut uses = placements.uses.clone();
        for (setup, list) in weenies {
            let u = uses.entry(*setup).or_default();
            let mut names: BTreeMap<&str, usize> = BTreeMap::new();
            for n in list.iter().filter_map(|f| f.name.as_deref()) {
                *names.entry(n).or_default() += 1;
            }
            let mut names: Vec<(&str, usize)> = names.into_iter().collect();
            names.sort_by(|a, b| {
                b.1.cmp(&a.1)
                    .then_with(|| a.0.len().cmp(&b.0.len()))
                    .then_with(|| a.0.cmp(b.0))
            });
            u.weenies = names.into_iter().map(|(n, _)| n.to_owned()).collect();
            u.weenie_count = u32::try_from(list.len()).unwrap_or(u32::MAX);
            let mut counts: BTreeMap<Category, usize> = BTreeMap::new();
            for f in list {
                *counts.entry(Category::of_weenie(f)).or_default() += 1;
            }
            u.weenie_category = counts
                .into_iter()
                .max_by_key(|(c, n)| (*n, std::cmp::Reverse(*c)))
                .map(|(c, _)| c);
        }
        let mut out = Self {
            uses,
            categories: BTreeMap::new(),
            labels: BTreeMap::new(),
            lower_labels: BTreeMap::new(),
            words: BTreeMap::new(),
        };
        out.resolve();
        out
    }

    /// Each model's category, then its label (a part's after its setup's).
    fn resolve(&mut self) {
        for (m, u) in &self.uses {
            let placed = u.buildings + u.outdoors + u.inside;
            let c = if !u.scenery.is_empty() {
                shape_category(u.size, u.aligned, u.cut_out)
            } else if u.buildings > 0 {
                Category::Building
            } else if let Some(c) = u.weenie_category.filter(|_| u.weenie_count >= placed) {
                c
            } else if u.inside > u.outdoors {
                Category::Furniture
            } else if u.outdoors > 0 {
                Category::BuildingPiece
            } else if !u.parents.is_empty() {
                Category::Part
            } else {
                Category::Other
            };
            self.categories.insert(*m, c);
        }
        let own: BTreeMap<u32, String> = self
            .uses
            .iter()
            .filter_map(|(m, u)| Some((*m, self.own_label(*m, u)?)))
            .collect();
        // A part is named by the setup it is part of that says most: one weenies are drawn
        // with first, then the one the world places most.
        let rank = |p: &u32| {
            self.uses.get(p).map_or((false, 0), |u| {
                (!u.weenies.is_empty(), u.buildings + u.outdoors + u.inside)
            })
        };
        for (m, u) in &self.uses {
            let label = own.get(m).cloned().or_else(|| {
                let parent = u
                    .parents
                    .iter()
                    .filter(|p| own.contains_key(p))
                    .max_by_key(|p| (rank(p), std::cmp::Reverse(**p)))?;
                Some(format!("Part of {}", own[parent]))
            });
            if let Some(l) = label {
                self.labels.insert(*m, l);
            }
        }
        for (m, l) in &self.labels {
            self.lower_labels.insert(*m, l.to_lowercase());
        }
        // A part answers to its own category, not to the name of what it is part of.
        for (m, u) in &self.uses {
            let c = self.category(DataId(*m));
            let label = self.labels.get(m).filter(|_| c != Category::Part);
            let words: Vec<&str> = std::iter::once(c.name())
                .chain(label.map(String::as_str))
                .chain(u.weenies.iter().map(String::as_str))
                .chain(u.scenery.iter().map(String::as_str))
                .chain(u.towns.iter().map(String::as_str))
                .collect();
            self.words.insert(*m, words.join("\n").to_lowercase());
        }
    }

    /// The label model `m` has by its own use, not its setups'.
    fn own_label(&self, m: u32, u: &ModelUse) -> Option<String> {
        let c = self.categories.get(&m).copied().unwrap_or(Category::Other);
        let kind = c.name().to_lowercase();
        if let Some(t) = u.scenery.first() {
            return Some(format!("{t} scenery: {kind}"));
        }
        let placed = matches!(
            c,
            Category::Building | Category::BuildingPiece | Category::Furniture
        );
        if let Some(first) = u.weenies.first().filter(|_| !placed) {
            return Some(match (u.weenies.len(), common_word(&u.weenies)) {
                (1, _) => first.clone(),
                (n, Some(w)) => format!("{w} ({n} weenies)"),
                (n, None) => format!("{first} +{}", n - 1),
            });
        }
        if placed {
            return Some(match u.towns.first() {
                Some(t) => format!("{t} {kind}"),
                None => match u.landblocks {
                    1 => format!("{} in a landblock", c.name()),
                    n => format!("{} in {n} landblocks", c.name()),
                },
            });
        }
        None
    }

    /// Model `id`'s use, when the world uses it.
    #[must_use]
    pub fn get(&self, id: DataId) -> Option<&ModelUse> {
        self.uses.get(&id.raw())
    }

    /// The category model `id` is in.
    #[must_use]
    pub fn category(&self, id: DataId) -> Category {
        self.categories
            .get(&id.raw())
            .copied()
            .unwrap_or(Category::Other)
    }

    /// The few words the browser shows beside model `id`: its weenie's name (or the word most of
    /// its weenies' names share, `Drudge (72 weenies)`), else what it is and where (`Grassland
    /// scenery: tree`, `Holtburg building piece`), else the setup it is a part of (`Part of
    /// Drudge (72 weenies)`).
    #[must_use]
    pub fn label(&self, id: DataId) -> Option<String> {
        self.labels.get(&id.raw()).cloned()
    }

    /// Whether model `id` answers the browser's filter `filter`: its id in hexadecimal, or its
    /// label (but a part's), its category, any of its weenies', terrains' or towns' names, or one
    /// of `tags`, holds the text, whatever its case.
    #[must_use]
    pub fn matches(&self, id: DataId, filter: &str, tags: &[String]) -> bool {
        self.matches_lower(id, &filter.trim().to_lowercase(), tags)
    }

    /// [`Self::matches`] for a filter already trimmed and in lower case, as a list asks it of
    /// every model.
    #[must_use]
    pub fn matches_lower(&self, id: DataId, f: &str, tags: &[String]) -> bool {
        if f.is_empty() {
            return true;
        }
        // Its id in hexadecimal, which only a filter of hexadecimal digits can be in.
        let digits = f.trim_start_matches("0x");
        if digits.bytes().all(|b| b.is_ascii_hexdigit())
            && format!("{:08x}", id.raw()).contains(digits)
        {
            return true;
        }
        if tags.iter().any(|t| t.to_lowercase().contains(f)) {
            return true;
        }
        match self.words.get(&id.raw()) {
            Some(words) => words.contains(f),
            None => self.category(id).name().to_lowercase().contains(f),
        }
    }

    /// Whether model `id`'s label holds `lower`, a text in lower case.
    #[must_use]
    pub fn label_holds(&self, id: DataId, lower: &str) -> bool {
        self.lower_labels
            .get(&id.raw())
            .is_some_and(|l| l.contains(lower))
    }
}

/// The tags a user gives models, kept in [`Tags::FILE`] beside the world file: each model's id,
/// then the words it is found by.
#[derive(Clone, Debug)]
pub struct Tags {
    path: PathBuf,
    doc: toml_edit::DocumentMut,
    tags: BTreeMap<u32, Vec<String>>,
}

impl Tags {
    /// The file's name.
    pub const FILE: &'static str = "tags.toml";

    /// The tags kept beside the world file `world` (none when the file is not there).
    ///
    /// # Errors
    /// The file is there but cannot be read, or is not a tags file.
    pub fn read(world: &Path) -> Result<Self, String> {
        let path = world
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(Self::FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::from(
                "# The world editor's tags: a model's id, then the words the model browser finds \
                 it by.\n\n[models]\n",
            ),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut tags = BTreeMap::new();
        if let Some(models) = doc.get("models").and_then(toml_edit::Item::as_table_like) {
            for (k, v) in models.iter() {
                let Some(id) = k
                    .strip_prefix("0x")
                    .or_else(|| k.strip_prefix("0X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                else {
                    return Err(format!("{}: `{k}` is not a model's id", path.display()));
                };
                let list: Vec<String> = v
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                tags.insert(id, list);
            }
        }
        Ok(Self { path, doc, tags })
    }

    /// The file the tags are kept in.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Model `id`'s tags, in the order they were given.
    #[must_use]
    pub fn of(&self, id: DataId) -> &[String] {
        self.tags.get(&id.raw()).map_or(&[], Vec::as_slice)
    }

    /// Every tag, with how many models have it.
    #[must_use]
    pub fn all(&self) -> BTreeMap<String, usize> {
        let mut out = BTreeMap::new();
        for t in self.tags.values().flatten() {
            *out.entry(t.clone()).or_default() += 1;
        }
        out
    }

    /// Give model `id` the tag `tag` (trimmed), and save. False when it had it already or the
    /// tag is empty.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn add(&mut self, id: DataId, tag: &str) -> Result<bool, String> {
        let tag = tag.trim();
        if tag.is_empty() || self.of(id).iter().any(|t| t == tag) {
            return Ok(false);
        }
        self.tags.entry(id.raw()).or_default().push(tag.to_owned());
        self.write(id)?;
        Ok(true)
    }

    /// Take the tag `tag` from model `id`, and save. False when it did not have it.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn remove(&mut self, id: DataId, tag: &str) -> Result<bool, String> {
        let Some(list) = self.tags.get_mut(&id.raw()) else {
            return Ok(false);
        };
        let before = list.len();
        list.retain(|t| t != tag);
        if list.len() == before {
            return Ok(false);
        }
        if list.is_empty() {
            self.tags.remove(&id.raw());
        }
        self.write(id)?;
        Ok(true)
    }

    /// Model `id`'s line put as it is now, and the file saved.
    fn write(&mut self, id: DataId) -> Result<(), String> {
        let key = format!("0x{:08X}", id.raw());
        if !self.doc.contains_key("models") {
            self.doc["models"] = toml_edit::table();
        }
        let models = self.doc["models"]
            .as_table_like_mut()
            .ok_or_else(|| format!("{}: `models` is not a table", self.path.display()))?;
        match self.tags.get(&id.raw()) {
            Some(list) => {
                let a: toml_edit::Array = list.iter().map(String::as_str).collect();
                models.insert(&key, toml_edit::value(a));
            }
            None => {
                models.remove(&key);
            }
        }
        std::fs::write(&self.path, self.doc.to_string())
            .map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the model browser's filter)
    use super::*;

    #[test]
    fn a_filter_finds_a_model_by_any_of_its_words_whatever_its_case_and_a_part_not_by_its_setup() {
        let mut uses = BTreeMap::new();
        uses.insert(
            0x0200_0001,
            ModelUse {
                scenery: vec!["Lush Grass".into()],
                towns: vec!["Holtburg".into()],
                outdoors: 3,
                landblocks: 1,
                ..ModelUse::default()
            },
        );
        uses.insert(
            0x0100_0002,
            ModelUse {
                parents: vec![0x0200_0001],
                ..ModelUse::default()
            },
        );
        let weenies = BTreeMap::from([(
            0x0200_0001,
            vec![WeenieFacts {
                name: Some("Drudge Skulker".into()),
                ..WeenieFacts::default()
            }],
        )]);
        let names = ModelNames::new(&Placements { uses }, &weenies);
        let (setup, part) = (DataId(0x0200_0001), DataId(0x0100_0002));
        for f in [
            "lush",
            "GRASS",
            "holtburg",
            "skulker",
            "0001",
            "0x02000001",
            "",
        ] {
            assert!(names.matches(setup, f, &[]), "{f}");
        }
        assert!(!names.matches(setup, "olthoi", &[]));
        assert!(names.matches(setup, "inn", &["Inn".into()]));
        // A part answers to its own kind, and to its id, not to its setup's label.
        let label = names.label(part).unwrap();
        assert!(label.starts_with("Part of"), "{label}");
        assert!(names.label_holds(part, "part of"));
        assert!(!names.matches(part, "lush", &[]));
        assert!(names.matches(part, "part", &[]) && names.matches(part, "0002", &[]));
        // A model the world does not use answers to its kind.
        assert!(names.matches(DataId(0x0200_0003), "other", &[]));
        assert!(!names.matches(DataId(0x0200_0003), "grass", &[]));
    }
}
