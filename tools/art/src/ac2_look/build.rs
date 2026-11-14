//! Building the look: the ticked parts written as one pack's folder, from the player's Asheron's
//! Call 2 files (every picture and vertex) and Asheron's Call files (the rocks' collision) and the
//! [`tables`](super::tables), then packed into one file as `dereth-art pack` packs it.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use auberean_assets::region::MaterialGroup;

use super::model::{self, Gltf, Recipe};
use super::picture;
use super::source::{self, Ac2, Rgba};
use super::tables::{Fit, Model, Part, Tables};

/// The pack's id.
pub const PACK_ID: &str = "local.ac2-look";
/// The pack's name.
pub const PACK_NAME: &str = "Asheron's Call 2 Art Pack";

/// Who made what the pack draws, with what, under which terms: every entry's credit.
const CREDIT: &str = "author = \"Turbine (Asheron's Call 2)\"\ntool = \"dereth-art ac2-look\"\n\
license = \"made from the player's own Asheron's Call 2 files, for their own computer\"\n";

/// What to build.
#[derive(Debug, Clone)]
pub struct Request {
    /// The Asheron's Call 2 install.
    pub ac2: PathBuf,
    /// The Asheron's Call files (needed for the rocks).
    pub ac1: Option<PathBuf>,
    /// The packed file to write.
    pub out: PathBuf,
    /// The parts to build.
    pub parts: BTreeSet<Part>,
    /// The creatures to build, by name.
    pub creatures: BTreeSet<String>,
    /// Keep the pack's folder beside the file, not only the file.
    pub keep_folder: bool,
}

/// What a build wrote.
#[derive(Debug)]
pub struct Built {
    pub out: PathBuf,
    /// The pack's version: the tables' ([`version`]).
    pub version: String,
    pub records: usize,
    pub bytes: u64,
    /// What the build left out, a line each: a ground or a model whose records would not read.
    pub left_out: Vec<String>,
}

/// The look's version: the sum of every table's version, so it goes up whenever any table does.
#[must_use]
pub fn version(t: &Tables) -> String {
    let sum: u32 = [
        t.terrain.version,
        t.trees.version,
        t.rocks.version,
        t.sky.version,
        t.creatures.version,
    ]
    .iter()
    .sum();
    format!("0.1.{sum}")
}

/// The folder a packed file's pack is written in while it is built: beside it.
#[must_use]
pub fn folder_of(out: &Path) -> PathBuf {
    let name = out
        .file_stem()
        .map_or_else(|| "pack".into(), |n| n.to_string_lossy().into_owned());
    out.with_file_name(format!("{name}.folder"))
}

/// A number as the folder names write it: as short as it reads back.
fn short(v: f64) -> String {
    format!("{v}")
}

/// The pack's folder, written file by file.
struct Folder {
    root: PathBuf,
}

impl Folder {
    fn write(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let at = self.root.join(path);
        if let Some(d) = at.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(&at, bytes).map_err(|e| format!("{}: {e}", at.display()))
    }

    fn picture(&self, path: &str, p: &Rgba) -> Result<(), String> {
        self.write(path, &picture::png(p)?)
    }

    fn model(&self, dir: &str, g: &Gltf) -> Result<(), String> {
        self.model_named(dir, "model", g)
    }

    /// `g` written as `{dir}/{name}.gltf`, its buffer `{name}.bin` beside it, with its pictures.
    fn model_named(&self, dir: &str, name: &str, g: &Gltf) -> Result<(), String> {
        for (file, p) in &g.pictures {
            self.picture(&format!("{dir}/{file}"), p)?;
        }
        self.write(&format!("{dir}/{name}.bin"), &g.bin)?;
        let mut json = g.json.clone();
        json["buffers"][0]["uri"] = serde_json::Value::String(format!("{name}.bin"));
        let text = serde_json::to_string_pretty(&json).map_err(|e| e.to_string())?;
        self.write(&format!("{dir}/{name}.gltf"), text.as_bytes())
    }
}

/// A group's coloring relative to `reference`, red, green and blue; `None` when it has none.
fn tint_of(g: &MaterialGroup, reference: [u8; 3]) -> Option<[f64; 3]> {
    let c = g.vertex_colors.first()?.color.0;
    let rgb = [(c >> 16) & 255, (c >> 8) & 255, c & 255];
    Some([0, 1, 2].map(|k| f64::from(rgb[k]) / f64::from(reference[k])))
}

/// A ground row's detail picture, as the manifest names it, and the meters a repeat of it covers.
type Detail = Option<(String, f32)>;

/// The ground: each terrain type's flat and steep pictures and the road's, with their relief, and
/// each one's detail picture when the table asks for them.
#[allow(clippy::too_many_lines)]
fn ground(
    ac2: &Ac2,
    t: &Tables,
    parts: &BTreeSet<Part>,
    folder: &Folder,
    manifest: &mut String,
    left_out: &mut Vec<String>,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let region = ac2.region()?;
    let terrain = parts.contains(&Part::Terrain);
    let red = parts.contains(&Part::RedDesert);
    let limit = t.terrain.picture_limit;
    // Each picture written: (file stem, tinted) to the file, its tile, and its relief maps.
    let mut done: BTreeMap<(String, bool), Option<String>> = BTreeMap::new();
    let mut stems: BTreeMap<String, bool> = BTreeMap::new();
    let mut write_ground = |stem: &str,
                            list: u32,
                            tint: Option<[f64; 3]>,
                            progress: &mut dyn FnMut(&str)|
     -> Result<Option<String>, String> {
        let key = (stem.to_owned(), tint.is_some());
        if let Some(d) = done.get(&key) {
            return Ok(d.clone());
        }
        // A stem drawn both plain and tinted keeps the plain one's name.
        let name = match stems.get(stem) {
            Some(&was) if was != tint.is_some() => format!("{stem}-red"),
            _ => stem.to_owned(),
        };
        stems.entry(stem.to_owned()).or_insert(tint.is_some());
        progress(&format!("ground {name}"));
        let made = match ac2.picture(list, limit) {
            None => None,
            Some(p) => {
                let p = match tint {
                    Some(k) => picture::tinted(&p, k),
                    None => p,
                };
                let (height, normal) = picture::relief(&p, t.terrain.relief_strength);
                folder.picture(&format!("ground/{name}.png"), &p)?;
                folder.picture(&format!("ground/{name}_height.png"), &height)?;
                folder.picture(&format!("ground/{name}_normal.png"), &normal)?;
                Some(name)
            }
        };
        done.insert(key, made.clone());
        Ok(made)
    };
    // Each detail picture written once, by its picture list: its file, or none where it would not
    // read.
    let mut details: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut detail_of =
        |group: &MaterialGroup, progress: &mut dyn FnMut(&str)| -> Result<Detail, String> {
            if !t.terrain.detail {
                return Ok(None);
            }
            let Some(d) = group.detail.first() else {
                return Ok(None);
            };
            let list = ac2.ground_picture(d.material.0);
            if list == 0 {
                return Ok(None);
            }
            let file = match details.get(&list) {
                Some(f) => f.clone(),
                None => {
                    let file = format!("ground/detail_{list:08X}.png");
                    progress(&format!("detail {list:08X}"));
                    let made = match ac2.picture(list, limit) {
                        Some(p) => {
                            folder.picture(&file, &p)?;
                            Some(file)
                        }
                        None => None,
                    };
                    details.insert(list, made.clone());
                    made
                }
            };
            let size = if d.tile_width > 0.0 {
                d.tile_width
            } else {
                1.0
            };
            Ok(file.map(|f| (f, size)))
        };
    let row = |manifest: &mut String,
               steep: bool,
               ground: &str,
               name: &str,
               kind: &str,
               tile: f32,
               detail: &Detail| {
        let size = if tile == 0.0 { 4.0 } else { tile };
        let detail = detail.as_ref().map_or_else(String::new, |(file, size)| {
            format!("detail = \"{file}\"\ndetail_size = {size}\n")
        });
        // A flat material's grounds' slopes follow it.
        let pebbles = if steep || t.terrain.pebbles {
            ""
        } else {
            "pebbles = false\n"
        };
        let _ = write!(
            manifest,
            "# AC2 {kind}\n[[{}]]\nground = {ground}\nbase_color = \"ground/{name}.png\"\n\
             normal = \"ground/{name}_normal.png\"\nheight = \"ground/{name}_height.png\"\n\
             size = {size}\n{detail}{pebbles}{CREDIT}\n",
            if steep { "steep" } else { "terrain" }
        );
    };
    for g in &t.terrain.ground {
        let desert = t.terrain.red_desert.types.contains(&g.ground_type);
        if !(terrain || (red && desert)) {
            continue;
        }
        let Some(kind) = region.terrain.terrains.iter().find(|k| k.index == g.kind) else {
            left_out.push(format!(
                "ground {}: no terrain kind {}",
                g.ground_type, g.kind
            ));
            continue;
        };
        let Some(surface) = region
            .surface
            .surfaces
            .iter()
            .find(|s| s.index == kind.surface)
        else {
            left_out.push(format!(
                "ground {}: {} has no surface",
                g.ground_type, kind.name
            ));
            continue;
        };
        let groups = &surface.groups;
        if groups.is_empty() {
            left_out.push(format!(
                "ground {}: {} has no groups",
                g.ground_type, kind.name
            ));
            continue;
        }
        let flat = g.flat.unwrap_or(0);
        let steep_at = g.steep.unwrap_or(groups.len() - 1);
        // The flat row laid, and whether a steep one was: a kind with one group that lays its flat
        // picture on its slopes too gets a steep row of the same.
        let mut flat_row: Option<(String, f32, Detail)> = None;
        let mut steep_row = false;
        for (steep, at) in [
            (false, Some(flat)),
            (true, (steep_at != flat).then_some(steep_at)),
        ] {
            let Some(group) = at.and_then(|k| groups.get(k)) else {
                continue;
            };
            let Some(base) = group.base.first() else {
                continue;
            };
            let list = ac2.ground_picture(base.material.0);
            if list == 0 {
                left_out.push(format!(
                    "ground {} {}: its material draws no picture",
                    g.ground_type, kind.name
                ));
                continue;
            }
            let stem = if steep {
                format!("{}-steep", kind.name)
            } else {
                kind.name.clone()
            };
            let tint = if red && desert {
                tint_of(group, t.terrain.red_desert.reference)
            } else {
                None
            };
            if let Some(name) = write_ground(&stem, list, tint, progress)? {
                let detail = detail_of(group, progress)?;
                row(
                    manifest,
                    steep,
                    &format!("[{}]", g.ground_type),
                    &name,
                    &kind.name,
                    base.tile_width,
                    &detail,
                );
                if steep {
                    steep_row = true;
                } else {
                    flat_row = Some((name, base.tile_width, detail));
                }
            }
        }
        if let (true, false, Some((name, tile, detail))) = (g.steep_flat, steep_row, flat_row) {
            row(
                manifest,
                true,
                &format!("[{}]", g.ground_type),
                &name,
                &kind.name,
                tile,
                &detail,
            );
        }
    }
    if terrain {
        let road = region
            .surface
            .roads
            .iter()
            .find_map(|r| r.groups.first().and_then(|g| Some((g, g.base.first()?))));
        if let Some((group, base)) = road {
            let list = ac2.ground_picture(base.material.0);
            if let Some(name) = write_ground("road", list, None, progress)? {
                let detail = detail_of(group, progress)?;
                row(
                    manifest,
                    false,
                    "\"road\"",
                    &name,
                    "road",
                    base.tile_width,
                    &detail,
                );
            }
        }
    }
    Ok(())
}

/// The folder a model is written in, by what it is made from.
fn model_dir(m: &Model) -> String {
    let mut d = format!("{:08X}-{}", m.entity, short(m.height));
    match m.fit {
        Some(f @ (Fit::Girth | Fit::Narrow)) => {
            let name = if f == Fit::Girth { "girth" } else { "narrow" };
            let _ = write!(d, "-{name}{}", short(m.cylinder.unwrap_or(0.0)));
        }
        Some(Fit::Inside) => d.push_str("-inside"),
        Some(Fit::Spheres) => d.push_str("-spheres"),
        None => {}
    }
    d
}

fn recipe(t: &Tables, height: f64, foliage: bool) -> Recipe {
    Recipe {
        height,
        foliage,
        cylinder: 0.0,
        fit: None,
        upright: false,
        picture_limit: t.terrain.picture_limit,
        clear_share: t.trees.clear_share,
        clear_size: t.trees.clear_size,
    }
}

/// A model's level's file name in its folder, without its extension: `model` for its most
/// detailed, `lod1`, `lod2`, ... for the coarser.
fn level_name(k: usize) -> String {
    if k == 0 {
        "model".to_owned()
    } else {
        format!("lod{k}")
    }
}

/// One model as its table row says: its trunk fitted, or fitted inside its collision. A tree
/// comes with every level Asheron's Call 2 draws it at when the table says so, each with how far
/// from the eye it is drawn out to (`None` for the last); anything else with its most detailed
/// alone.
fn build_model(
    ac2: &Ac2,
    t: &Tables,
    m: &Model,
    ac1: Option<&dereth_dat::RetailDatStore>,
) -> Result<Vec<(Option<f64>, Gltf)>, String> {
    let one = |g: Gltf| vec![(None, g)];
    let base = recipe(t, m.height, m.foliage);
    let shape = |store: Option<&dereth_dat::RetailDatStore>| -> Result<source::Shape, String> {
        let store = store.ok_or("the rocks need the Asheron's Call files")?;
        source::collision(store, m.setup, t.rocks.solid_spheres)
    };
    match m.fit {
        Some(Fit::Inside) => {
            let shape = shape(ac1)?;
            let (mut g, _) = model::static_model(ac2, m.entity, &base)?;
            model::refit(&mut g, &shape, t.rocks.tolerance);
            Ok(one(g))
        }
        Some(Fit::Spheres) => {
            let shape = shape(ac1)?;
            let mut parts = Vec::new();
            let mut turned = false;
            for sphere in &shape.spheres {
                let big = sphere[3] >= 2.0;
                let entity = if big { m.entity } else { t.rocks.small_rock };
                let (mut g, _) =
                    model::static_model(ac2, entity, &recipe(t, 2.0 * sphere[3], false))?;
                if big && turned {
                    model::turn_half(&mut g);
                }
                turned = turned || big;
                let one = source::Shape {
                    cylinders: Vec::new(),
                    spheres: vec![*sphere],
                };
                model::refit(&mut g, &one, t.rocks.tolerance);
                parts.push(g);
            }
            Ok(one(model::merge(&parts)))
        }
        fit => {
            let r = Recipe {
                cylinder: m.cylinder.unwrap_or(0.0),
                fit,
                ..base
            };
            let levels = t.trees.levels && m.category == super::tables::Category::Tree;
            model::static_levels(ac2, m.entity, &r, levels).map(|(g, _)| g)
        }
    }
}

/// The creatures asked for: each body, its pictures, its looks and its clips under
/// `creatures/<name>/<body>/`, its `[[skin]]` rows, and the weenies every one keeps as Asheron's
/// Call draws them.
fn creatures(
    ac2: &Ac2,
    t: &Tables,
    names: &BTreeSet<String>,
    folder: &Folder,
    manifest: &mut String,
    left_out: &mut Vec<String>,
    progress: &mut dyn FnMut(&str),
) -> Result<(), String> {
    use super::creature;
    let mut weenies: BTreeSet<u32> = BTreeSet::new();
    let mut launchers = false;
    for c in t
        .creatures
        .creature
        .iter()
        .filter(|c| names.contains(&c.name))
    {
        let dir = format!("creatures/{}", c.name);
        let mut made_bodies: BTreeSet<&str> = BTreeSet::new();
        for b in &c.body {
            progress(&format!("creature {} {}", c.name, b.folder));
            let made = match creature::make(ac2, b) {
                Ok(m) => m,
                Err(e) => {
                    left_out.push(format!("creature {} {}: {e}", c.name, b.folder));
                    continue;
                }
            };
            let file = creature::body_file(&made, b);
            let at = format!("{dir}/{}", b.folder);
            let mut own: BTreeMap<u32, Rgba> = BTreeMap::new();
            for list in &file.images {
                if own.contains_key(list) {
                    continue;
                }
                let p = ac2.picture(*list, creature::PICTURE_LIMIT).ok_or_else(|| {
                    format!("creature {}: picture {list:08X} will not read", c.name)
                })?;
                folder.picture(&format!("{at}/tex_{list:08X}.png"), &p)?;
                own.insert(*list, p);
            }
            folder.write(&format!("{at}/body.bin"), &file.bin)?;
            let text = serde_json::to_string(&file.json).map_err(|e| e.to_string())?;
            folder.write(&format!("{at}/body.gltf"), text.as_bytes())?;
            // The differences between the body's pictures, which a masked look's tint is laid by.
            let mask = match &b.mask {
                Some(lists) => {
                    let mut pictures = Vec::new();
                    for list in lists {
                        let p = match own.get(list) {
                            Some(p) => p.clone(),
                            None => {
                                ac2.picture(*list, creature::PICTURE_LIMIT).ok_or_else(|| {
                                    format!("creature {}: picture {list:08X} will not read", c.name)
                                })?
                            }
                        };
                        pictures.push(picture::opaque(&p));
                    }
                    Some(picture::differences(&pictures))
                }
                None => None,
            };
            for l in c.look.iter().filter(|l| l.body == b.folder) {
                match creature::look_files(ac2, l, &file, &own, mask.as_ref()) {
                    Ok(look) => {
                        for (name, p) in &look.pictures {
                            folder.picture(&format!("{at}/{name}"), p)?;
                        }
                        let text = serde_json::to_string(&look.json).map_err(|e| e.to_string())?;
                        folder.write(&format!("{at}/{}.gltf", l.name), text.as_bytes())?;
                    }
                    Err(e) => left_out.push(format!("creature {}: {e}", c.name)),
                }
            }
            made_bodies.insert(b.folder.as_str());
        }
        let rows = super::tables::Creature {
            skin: c
                .skin
                .iter()
                .filter(|r| made_bodies.contains(r.body.as_str()))
                .cloned()
                .collect(),
            ..c.clone()
        };
        let _ = write!(
            manifest,
            "# AC2's {} in place of Asheron's Call's\n{}\n",
            c.label,
            creature::rows(&rows, &dir, CREDIT.trim_end())
        );
        weenies.extend(&c.keep_weenies);
        launchers |= c.keep_launchers;
    }
    if !weenies.is_empty() || launchers {
        let list: Vec<String> = weenies.iter().map(u32::to_string).collect();
        let _ = write!(
            manifest,
            "# Kept as Asheron's Call draws them\n[keep]\nlaunchers = {launchers}\nweenies = [{}]\n\n",
            list.join(", ")
        );
    }
    Ok(())
}

/// What the installed pack says it is: what it was made from, then a line of the parts it holds.
fn description(parts: &BTreeSet<Part>, creatures: bool) -> String {
    let mut labels: Vec<&str> = parts.iter().map(|p| p.label()).collect();
    if creatures {
        labels.push("Creatures");
    }
    format!(
        "Asheron's Call 2 art pack generated from your own data set.\nIncludes: {}",
        labels.join(", ")
    )
}

/// The pack's `[pack]` table.
fn head(t: &Tables, parts: &BTreeSet<Part>, creatures: bool) -> String {
    format!(
        "[pack]\nid = \"{PACK_ID}\"\nversion = \"{}\"\nname = \"{PACK_NAME}\"\nformat = 1\n\
         description = {}\n\n",
        version(t),
        toml::Value::String(description(parts, creatures))
    )
}

/// Build the look as `req` says, telling `progress` what it is doing a line at a time.
///
/// # Errors
/// An install will not open, a file will not write, or the pack will not pack.
#[allow(clippy::too_many_lines)]
pub fn build(t: &Tables, req: &Request, progress: &mut dyn FnMut(&str)) -> Result<Built, String> {
    if req.parts.is_empty() && req.creatures.is_empty() {
        return Err("no part of the look was asked for".into());
    }
    for name in &req.creatures {
        if t.creatures.named(name).is_none() {
            return Err(format!("no creature {name}"));
        }
    }
    let ac2 = Ac2::open(&req.ac2)?;
    let ac1 = if req.parts.contains(&Part::Rocks) {
        let dir = req
            .ac1
            .as_deref()
            .ok_or("the rocks need the Asheron's Call files")?;
        Some(crate::survey::open_dats(dir)?)
    } else {
        None
    };
    let root = folder_of(&req.out);
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    let folder = Folder { root: root.clone() };
    let mut manifest = head(t, &req.parts, !req.creatures.is_empty());
    let mut left_out = Vec::new();
    if req.parts.contains(&Part::Terrain) || req.parts.contains(&Part::RedDesert) {
        ground(
            &ac2,
            t,
            &req.parts,
            &folder,
            &mut manifest,
            &mut left_out,
            progress,
        )?;
    }
    let mut models: Vec<&Model> = Vec::new();
    if req.parts.contains(&Part::Trees) {
        models.extend(&t.trees.model);
    }
    if req.parts.contains(&Part::Rocks) {
        models.extend(&t.rocks.model);
    }
    // Each model's folder written once, with how far each of its levels is drawn out to.
    let mut written: BTreeMap<String, Vec<Option<f64>>> = BTreeMap::new();
    for m in models {
        let dir = model_dir(m);
        if !written.contains_key(&dir) {
            progress(&format!("model {dir}"));
            match build_model(&ac2, t, m, ac1.as_ref()) {
                Ok(levels) => {
                    for (k, (_, g)) in levels.iter().enumerate() {
                        folder.model_named(&dir, &level_name(k), g)?;
                    }
                    written.insert(dir.clone(), levels.iter().map(|(d, _)| *d).collect());
                }
                Err(e) => {
                    left_out.push(format!("model {:#010X}: {e}", m.setup));
                    continue;
                }
            }
        }
        let _ = write!(
            manifest,
            "[[model]]\nid = \"0x{:08X}\"\ncategory = \"{}\"\nfoliage = {}\n{CREDIT}\n",
            m.setup,
            m.category.name(),
            m.foliage
        );
        for (k, distance) in written[&dir].iter().enumerate() {
            let _ = write!(
                manifest,
                "[[model.lod]]\nfile = \"{dir}/{}.gltf\"\n",
                level_name(k)
            );
            if let Some(d) = distance {
                let _ = writeln!(manifest, "distance = {}", short((d * 10.0).round() / 10.0));
            }
        }
        manifest.push('\n');
    }
    if req.parts.contains(&Part::Sky) {
        progress("sky");
        match ac2.picture(t.sky.clouds, t.terrain.picture_limit) {
            Some(p) => {
                folder.picture("sky/clouds.png", &p)?;
                let decks: Vec<String> = t
                    .sky
                    .decks
                    .iter()
                    .map(|d| format!("\"0x{d:08X}\""))
                    .collect();
                let _ = write!(
                    manifest,
                    "# AC2's clouds on the cloud decks\n[[material]]\ntexture = [{}]\n\
                     base_color = \"sky/clouds.png\"\n{CREDIT}\n",
                    decks.join(", ")
                );
            }
            None => left_out.push(format!("the clouds {:08X} will not read", t.sky.clouds)),
        }
    }
    if req.parts.contains(&Part::Trees) {
        let all = &t.trees.cover_all;
        let mut made: BTreeSet<u32> = BTreeSet::new();
        for c in &t.trees.cover {
            let dir = format!("cover/{:08X}", c.entity);
            if !made.contains(&c.entity) {
                progress(&format!("cover {:08X}", c.entity));
                let r = Recipe {
                    upright: true,
                    ..recipe(t, c.height, true)
                };
                match model::static_model(&ac2, c.entity, &r) {
                    Ok((g, _)) => {
                        folder.model(&dir, &g)?;
                        made.insert(c.entity);
                    }
                    Err(e) => {
                        left_out.push(format!("cover {:08X}: {e}", c.entity));
                        continue;
                    }
                }
            }
            let types: Vec<String> = c.types.iter().map(u8::to_string).collect();
            let _ = write!(
                manifest,
                "# AC2 frill {:08X}\n[[cover]]\nground = [{}]\ndensity = {:?}\n\
                 scale = [{:?}, {:?}]\nmax_slope = {:?}\nfoliage = false\ncategory = \"plant\"\n\
                 {CREDIT}\n[[cover.lod]]\nfile = \"{dir}/model.gltf\"\ndistance = {}\n\n",
                c.entity,
                types.join(", "),
                c.density,
                all.scale[0],
                all.scale[1],
                all.max_slope,
                all.reach
            );
        }
        for ty in &all.no_flowers {
            let _ = write!(manifest, "[lighting.grass.{ty}]\nflowers = 0\n\n");
        }
    }
    creatures(
        &ac2,
        t,
        &req.creatures,
        &folder,
        &mut manifest,
        &mut left_out,
        progress,
    )?;
    folder.write(dereth_assets::art_pack::MANIFEST_FILE, manifest.as_bytes())?;
    progress("packing");
    let report = crate::pack::pack(&root, &req.out)?;
    left_out.extend(report.problems);
    if !req.keep_folder {
        std::fs::remove_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    Ok(Built {
        out: report.out,
        version: version(t),
        records: report.records,
        bytes: report.bytes,
        left_out,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's build)
    use super::*;
    use crate::ac2_look::tables::Category;

    #[test]
    fn a_models_folder_names_its_entity_height_and_fit() {
        let m = Model {
            setup: 0x0200_0258,
            height: 24.0,
            entity: 0x4700_003E,
            category: Category::Tree,
            foliage: true,
            cylinder: Some(1.09),
            fit: Some(Fit::Girth),
        };
        assert_eq!(model_dir(&m), "4700003E-24-girth1.09");
        let rock = Model {
            height: 1.91,
            fit: Some(Fit::Inside),
            ..m.clone()
        };
        assert_eq!(model_dir(&rock), "4700003E-1.91-inside");
        let plain = Model {
            height: 0.35,
            fit: None,
            ..m
        };
        assert_eq!(model_dir(&plain), "4700003E-0.35");
    }

    #[test]
    fn the_installed_pack_is_named_and_lists_the_parts_it_holds() {
        let t = Tables::built_in().expect("the tables");
        let parts: BTreeSet<Part> = [Part::Sky, Part::Terrain, Part::Trees].into();
        let m = dereth_assets::art_pack::ArtPackManifest::parse(&head(&t, &parts, false))
            .expect("the manifest reads");
        assert_eq!(m.name, "Asheron's Call 2 Art Pack");
        assert_eq!(m.about.id, PACK_ID);
        assert_eq!(
            m.about.description,
            "Asheron's Call 2 art pack generated from your own data set.\n\
             Includes: Terrain, Trees and Ground Cover, Sky"
        );
    }

    #[test]
    fn the_looks_version_goes_up_with_any_tables() {
        let mut t = Tables::built_in().expect("the tables");
        let was = version(&t);
        t.sky.version += 1;
        assert_ne!(version(&t), was);
        assert_eq!(
            folder_of(Path::new("a/look.dat")),
            Path::new("a/look.folder")
        );
    }

    /// The big pine from this machine's AC2 install: its most detailed mesh alone while the table
    /// leaves AC2's levels off, and every level AC2 draws it at, each drawn out farther than the
    /// one before, once the table turns them on.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_tree_carries_ac2s_coarser_levels_only_when_the_table_asks() {
        let mut t = Tables::built_in().expect("the tables");
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("the install");
        let pine = t
            .trees
            .model
            .iter()
            .find(|m| m.setup == 0x0200_02D3)
            .cloned()
            .expect("the big pine's row");
        assert_eq!(pine.category, Category::Tree);
        t.trees.levels = false;
        let alone = build_model(&ac2, &t, &pine, None).expect("the pine builds");
        assert_eq!(alone.len(), 1);
        assert_eq!(alone[0].0, None, "drawn out to its picture");
        t.trees.levels = true;
        let levels = build_model(&ac2, &t, &pine, None).expect("the pine builds");
        assert!(levels.len() > 2, "{} levels", levels.len());
        let distances: Vec<Option<f64>> = levels.iter().map(|(d, _)| *d).collect();
        assert_eq!(distances.last(), Some(&None));
        let reach: Vec<f64> = distances.iter().flatten().copied().collect();
        assert_eq!(reach.len(), levels.len() - 1, "{distances:?}");
        assert!(reach.windows(2).all(|w| w[1] > w[0]), "{reach:?}");
    }

    /// The ground from this machine's AC2 install: every ground and the road carry the one detail
    /// picture AC2's ground names, at its own repeat, written once, and none when the table says;
    /// and every flat ground and the road keep the presentation's pebbles off, as the table says,
    /// and lay them again when it says not.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn every_ground_carries_its_detail_picture_and_keeps_pebbles_off_when_the_table_asks() {
        let mut t = Tables::built_in().expect("the tables");
        assert!(t.terrain.detail);
        assert!(!t.terrain.pebbles);
        let ac2 = auberean_dat::testing::require_target_install();
        let dir =
            std::env::temp_dir().join(format!("dereth-art-ac2-detail-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let manifest = |t: &Tables, name: &str| {
            let req = Request {
                ac2: ac2.clone(),
                ac1: None,
                out: dir.join(name),
                parts: [Part::Terrain].into(),
                creatures: BTreeSet::new(),
                keep_folder: true,
            };
            let built = build(t, &req, &mut |_| {});
            let root = folder_of(&req.out);
            let text = std::fs::read_to_string(root.join(dereth_assets::art_pack::MANIFEST_FILE));
            let details = std::fs::read_dir(root.join("ground")).map(|d| {
                d.filter_map(Result::ok)
                    .filter(|e| e.file_name().to_string_lossy().starts_with("detail_"))
                    .count()
            });
            (built.map(|b| b.left_out), text, details)
        };
        let with = manifest(&t, "with.dat");
        t.terrain.detail = false;
        t.terrain.pebbles = true;
        let without = manifest(&t, "without.dat");
        let _ = std::fs::remove_dir_all(&dir);
        let (left_out, text, details) = with;
        assert_eq!(left_out.expect("builds"), Vec::<String>::new());
        let m = dereth_assets::art_pack::ArtPackManifest::parse(&text.expect("a manifest"))
            .expect("reads");
        assert!(
            m.grounds.len() > t.terrain.ground.len(),
            "{}",
            m.grounds.len()
        );
        for g in &m.grounds {
            assert!(g.detail.is_some(), "{:?} has no detail", g.grounds);
            assert!((g.detail_size - 3.0).abs() < 1e-6, "{:?}", g.grounds);
            assert!(
                (g.detail_strength - 1.0).abs() < 1e-6,
                "{:?} at the detail as made",
                g.grounds
            );
            assert!(g.steep || !g.pebbles, "{:?} lays pebbles", g.grounds);
        }
        assert!(
            m.ground(dereth_assets::art_pack::Ground::Road).is_some(),
            "the road among them"
        );
        assert_eq!(details.expect("the ground folder"), 1);
        let (_, text, details) = without;
        let m = dereth_assets::art_pack::ArtPackManifest::parse(&text.expect("a manifest"))
            .expect("reads");
        assert!(m.grounds.iter().all(|g| g.detail.is_none()));
        assert!(m.grounds.iter().all(|g| g.pebbles));
        assert_eq!(details.expect("the ground folder"), 0);
    }

    /// Two creatures from this machine's AC2 install: a single-body one with per-look bodies and
    /// one with two bodies whose looks tint every picture; the pack opens as the client opens it,
    /// with nothing it cannot play, a body for every row the table has, each with an idle, and the
    /// weenies the creatures keep.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn creatures_build_from_the_install_into_bodies_the_client_plays() {
        let t = Tables::built_in().expect("the tables");
        let ac2 = auberean_dat::testing::require_target_install();
        let dir =
            std::env::temp_dir().join(format!("dereth-art-ac2-creatures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let req = Request {
            ac2,
            ac1: None,
            out: dir.join("creatures.dat"),
            parts: BTreeSet::new(),
            creatures: ["rabbit".to_owned(), "shreth".to_owned()].into(),
            keep_folder: false,
        };
        let built = build(&t, &req, &mut |_| {});
        let pack = match &built {
            Ok(b) => Some(dereth_scene::art_pack::ArtPack::open_path(&b.out)),
            Err(_) => None,
        };
        let _ = std::fs::remove_dir_all(&dir);
        let built = built.expect("the creatures build");
        assert!(built.left_out.is_empty(), "{:?}", built.left_out);
        let pack = pack.expect("a file").expect("the pack opens");
        assert!(pack.problems.is_empty(), "{:?}", pack.problems);
        let rows: usize = ["rabbit", "shreth"]
            .iter()
            .map(|n| t.creatures.named(n).expect("a creature").skin.len())
            .sum();
        assert_eq!(pack.skins.len(), rows);
        assert!(pack.skins.iter().all(|s| !s.clips.is_empty()));
        let rabbit = t.creatures.named("rabbit").expect("the rabbit");
        assert!(rabbit.keep_weenies.iter().all(|w| pack.keep.contains(w)));
    }

    /// The whole look from this machine's installs: nothing left out, every model and ground in
    /// the pack, a tree's trunk drawn in to its cylinder, and the same files give the same pack.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn the_whole_look_builds_from_the_installs_and_again_to_the_same_bytes() {
        let t = Tables::built_in().expect("the tables");
        let ac2 = auberean_dat::testing::require_target_install();
        let ac1 = dereth_dat::testing::dat_dir();
        let dir = std::env::temp_dir().join(format!("dereth-art-ac2-look-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a folder");
        let req = |out: &str, parts: &[Part]| Request {
            ac2: ac2.clone(),
            ac1: Some(ac1.clone()),
            out: dir.join(out),
            parts: parts.iter().copied().collect(),
            creatures: BTreeSet::new(),
            keep_folder: false,
        };
        let all = build(&t, &req("all.dat", &Part::ALL), &mut |_| {});
        let rocks = [Part::Rocks, Part::Sky];
        let once = build(&t, &req("a.dat", &rocks), &mut |_| {}).map(|b| std::fs::read(b.out));
        let twice = build(&t, &req("b.dat", &rocks), &mut |_| {}).map(|b| std::fs::read(b.out));
        let src = Ac2::open(&ac2).expect("the install");
        let tree = &t.trees.model[0];
        let made = model::static_model(
            &src,
            tree.entity,
            &Recipe {
                cylinder: tree.cylinder.unwrap_or(0.0),
                fit: tree.fit,
                ..recipe(&t, tree.height, tree.foliage)
            },
        );
        let _ = std::fs::remove_dir_all(&dir);
        let all = all.expect("the look builds");
        assert!(all.left_out.is_empty(), "{:?}", all.left_out);
        let models: BTreeSet<String> = t
            .trees
            .model
            .iter()
            .chain(&t.rocks.model)
            .map(model_dir)
            .collect();
        // The manifest, every model's glTF and buffer, its pictures, the grounds, the covers, the sky.
        assert!(all.records > models.len() * 2 + t.terrain.ground.len() + t.trees.cover.len() / 2);
        let (once, twice) = (once.expect("builds"), twice.expect("builds"));
        assert_eq!(once.expect("reads"), twice.expect("reads"));
        let (_, m) = made.expect("a tree");
        let (before, after) = m.girth.expect("a trunk");
        assert!(before > after, "{before} -> {after}");
        assert!(
            (after - tree.cylinder.unwrap_or(0.0)).abs() < 0.05,
            "{after}"
        );
    }
}
