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
        for (name, p) in &g.pictures {
            self.picture(&format!("{dir}/{name}"), p)?;
        }
        self.write(&format!("{dir}/model.bin"), &g.bin)?;
        let text = serde_json::to_string_pretty(&g.json).map_err(|e| e.to_string())?;
        self.write(&format!("{dir}/model.gltf"), text.as_bytes())
    }
}

/// A group's coloring relative to `reference`, red, green and blue; `None` when it has none.
fn tint_of(g: &MaterialGroup, reference: [u8; 3]) -> Option<[f64; 3]> {
    let c = g.vertex_colors.first()?.color.0;
    let rgb = [(c >> 16) & 255, (c >> 8) & 255, c & 255];
    Some([0, 1, 2].map(|k| f64::from(rgb[k]) / f64::from(reference[k])))
}

/// The ground: each terrain type's flat and steep pictures and the road's, with their relief.
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
    let row =
        |manifest: &mut String, steep: bool, ground: &str, name: &str, kind: &str, tile: f32| {
            let size = if tile == 0.0 { 4.0 } else { tile };
            let _ = write!(
                manifest,
                "# AC2 {kind}\n[[{}]]\nground = {ground}\nbase_color = \"ground/{name}.png\"\n\
             normal = \"ground/{name}_normal.png\"\nheight = \"ground/{name}_height.png\"\n\
             size = {size}\n{CREDIT}\n",
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
        let mut flat_row: Option<(String, f32)> = None;
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
                row(
                    manifest,
                    steep,
                    &format!("[{}]", g.ground_type),
                    &name,
                    &kind.name,
                    base.tile_width,
                );
                if steep {
                    steep_row = true;
                } else {
                    flat_row = Some((name, base.tile_width));
                }
            }
        }
        if let (true, false, Some((name, tile))) = (g.steep_flat, steep_row, flat_row) {
            row(
                manifest,
                true,
                &format!("[{}]", g.ground_type),
                &name,
                &kind.name,
                tile,
            );
        }
    }
    if terrain {
        let road = region
            .surface
            .roads
            .iter()
            .find_map(|r| r.groups.first().and_then(|g| g.base.first()));
        if let Some(base) = road {
            let list = ac2.ground_picture(base.material.0);
            if let Some(name) = write_ground("road", list, None, progress)? {
                row(manifest, false, "\"road\"", &name, "road", base.tile_width);
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

/// One model as its table row says: its trunk fitted, or fitted inside its collision.
fn build_model(
    ac2: &Ac2,
    t: &Tables,
    m: &Model,
    ac1: Option<&dereth_dat::RetailDatStore>,
) -> Result<Gltf, String> {
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
            Ok(g)
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
            Ok(model::merge(&parts))
        }
        fit => {
            let r = Recipe {
                cylinder: m.cylinder.unwrap_or(0.0),
                fit,
                ..base
            };
            model::static_model(ac2, m.entity, &r).map(|(g, _)| g)
        }
    }
}

/// What the installed pack says it is: what it was made from, then a line of the parts it holds.
fn description(parts: &BTreeSet<Part>) -> String {
    let labels: Vec<&str> = parts.iter().map(|p| p.label()).collect();
    format!(
        "Asheron's Call 2 art pack generated from your own data set.\nIncludes: {}",
        labels.join(", ")
    )
}

/// The pack's `[pack]` table.
fn head(t: &Tables, parts: &BTreeSet<Part>) -> String {
    format!(
        "[pack]\nid = \"{PACK_ID}\"\nversion = \"{}\"\nname = \"{PACK_NAME}\"\nformat = 1\n\
         description = {}\n\n",
        version(t),
        toml::Value::String(description(parts))
    )
}

/// Build the look as `req` says, telling `progress` what it is doing a line at a time.
///
/// # Errors
/// An install will not open, a file will not write, or the pack will not pack.
#[allow(clippy::too_many_lines)]
pub fn build(t: &Tables, req: &Request, progress: &mut dyn FnMut(&str)) -> Result<Built, String> {
    if req.parts.is_empty() {
        return Err("no part of the look was asked for".into());
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
    let mut manifest = head(t, &req.parts);
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
    let mut written: BTreeSet<String> = BTreeSet::new();
    for m in models {
        let dir = model_dir(m);
        if !written.contains(&dir) {
            progress(&format!("model {dir}"));
            match build_model(&ac2, t, m, ac1.as_ref()) {
                Ok(g) => {
                    folder.model(&dir, &g)?;
                    written.insert(dir.clone());
                }
                Err(e) => {
                    left_out.push(format!("model {:#010X}: {e}", m.setup));
                    continue;
                }
            }
        }
        let _ = write!(
            manifest,
            "[[model]]\nid = \"0x{:08X}\"\ncategory = \"{}\"\nfoliage = {}\n{CREDIT}\n\
             [[model.lod]]\nfile = \"{dir}/model.gltf\"\n\n",
            m.setup,
            m.category.name(),
            m.foliage
        );
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
        let m = dereth_assets::art_pack::ArtPackManifest::parse(&head(&t, &parts))
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
