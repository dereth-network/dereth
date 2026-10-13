//! The survey from start to finish: open the dats, walk the world, add the characters and the
//! server's spawns, read what every model is made of, draw its picture, and write the tables and
//! the report.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, TextureFormat};
use dereth_render_cpu::surface::surface_type;
use dereth_scene::textures::TextureStore;

use crate::character::{self, CharacterCounts};
use crate::model::{Facts, Models};
use crate::tally::{ModelId, Tally};
use crate::thumb::{self, Image, Material};
use crate::{output, spawns, world};

/// The picture size, in pixels.
pub const THUMBNAIL_SIZE: u32 = 128;

/// What to survey and where to write it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The folder holding the retail dats.
    pub dat_dir: PathBuf,
    /// The only folder written to.
    pub out: PathBuf,
    /// The server's spawn table, when there is one.
    pub spawns: Option<PathBuf>,
    /// Hand-made tags (`id<TAB>tags`); `tags.tsv` in `out` when it is there.
    pub tags: Option<PathBuf>,
    /// The landblocks to walk; every landblock when `None`.
    pub landblocks: Option<Vec<u16>>,
    pub threads: usize,
    /// Draw a picture of every model.
    pub thumbnails: bool,
}

/// Everything the survey found.
#[derive(Debug, Clone)]
pub struct Survey {
    pub tally: Tally,
    pub facts: BTreeMap<ModelId, Facts>,
    /// The models whose picture was written, by file name inside `out`.
    pub thumbnails: BTreeMap<ModelId, String>,
    pub tags: BTreeMap<ModelId, String>,
    pub characters: CharacterCounts,
    /// Rows read from the spawn table.
    pub spawn_rows: usize,
    /// Landblocks the spawn table places anything in.
    pub spawn_landblocks: u64,
    pub landblocks_walked: usize,
    /// The region's terrain type names, by terrain type.
    pub terrain_names: Vec<String>,
    pub seconds: f64,
}

/// Opens the retail dats in `dir`.
///
/// # Errors
/// The folder holds no retail dats, or they will not open.
pub fn open_dats(dir: &Path) -> Result<RetailDatStore, String> {
    let found = dereth_dat::locate_modern_dats(&[dir.to_path_buf()])
        .map_err(|e| format!("no retail dats in {}: {e}", dir.display()))?;
    found
        .open()
        .map_err(|e| format!("the dats in {} will not open: {e}", dir.display()))
}

/// Runs the survey and writes its output. `log` hears what it is doing.
///
/// # Errors
/// The dats will not open, an input will not read, or the output will not write.
pub fn run(opts: &Options, log: &(dyn Fn(&str) + Sync)) -> Result<Survey, String> {
    let started = Instant::now();
    let store = open_dats(&opts.dat_dir)?;
    let mut survey = collect(&store, opts, log)?;
    survey.seconds = started.elapsed().as_secs_f64();
    log(&format!(
        "writing the tables and the report to {}",
        opts.out.display()
    ));
    output::write(&survey, &opts.out)?;
    survey.seconds = started.elapsed().as_secs_f64();
    Ok(survey)
}

/// The survey over `store`, without writing the tables (the pictures are written, when asked
/// for, as they are drawn).
///
/// # Errors
/// An input will not read, or a picture will not write.
pub fn collect(
    store: &RetailDatStore,
    opts: &Options,
    log: &(dyn Fn(&str) + Sync),
) -> Result<Survey, String> {
    let started = Instant::now();
    let region = dereth_world_data::landblock::load_region(store)
        .map_err(|e| format!("the region will not load: {e}"))?;
    let landblocks = opts
        .landblocks
        .clone()
        .unwrap_or_else(world::every_landblock);
    log(&format!(
        "walking {} landblocks on {} threads",
        landblocks.len(),
        opts.threads
    ));
    let total = landblocks.len();
    let mut tally = world::walk(store, &region, &landblocks, opts.threads, &|done| {
        log(&format!("  {done} of {total} landblocks"));
    });
    log(&format!(
        "walked in {:.1} s: {} landblocks, {} models",
        started.elapsed().as_secs_f64(),
        tally.world.landblocks,
        tally.uses.len()
    ));
    let characters = character::survey(store, &mut tally);
    let mut spawn_rows = 0;
    let mut spawn_landblocks = 0;
    if let Some(path) = &opts.spawns {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let rows = spawns::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        spawn_rows = rows.len();
        spawn_landblocks = spawns::landblocks(&rows);
        spawns::fold(&mut tally, &rows);
    }
    let tags_path = opts
        .tags
        .clone()
        .or_else(|| Some(opts.out.join("tags.tsv")).filter(|p| p.is_file()));
    let tags = match &tags_path {
        Some(p) => read_tags(p)?,
        None => BTreeMap::new(),
    };
    let ids: Vec<ModelId> = tally.uses.keys().copied().collect();
    log(&format!(
        "reading {} models{}",
        ids.len(),
        if opts.thumbnails {
            " and drawing their pictures"
        } else {
            ""
        }
    ));
    let thumbs_dir = opts.out.join("thumbs");
    if opts.thumbnails {
        std::fs::create_dir_all(&thumbs_dir)
            .map_err(|e| format!("{}: {e}", thumbs_dir.display()))?;
    }
    let (facts, thumbnails) = describe(store, &tally, &ids, opts, &thumbs_dir)?;
    Ok(Survey {
        tally,
        facts,
        thumbnails,
        tags,
        characters,
        spawn_rows,
        spawn_landblocks,
        landblocks_walked: landblocks.len(),
        terrain_names: region
            .terrain_types
            .iter()
            .map(|t| t.terrain_name.clone())
            .collect(),
        seconds: started.elapsed().as_secs_f64(),
    })
}

/// Reads `id<TAB>tags` lines; blank lines and `#` lines are skipped.
///
/// # Errors
/// The file will not read, or a line's id is not an id.
pub fn read_tags(path: &Path) -> Result<BTreeMap<ModelId, String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_tags(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// [`read_tags`]' reading of the text.
///
/// # Errors
/// A line whose id is not an id.
pub fn parse_tags(text: &str) -> Result<BTreeMap<ModelId, String>, String> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        let (id, tags) = line.split_once('\t').unwrap_or((line, ""));
        if id.trim() == "id" {
            continue;
        }
        let id =
            ModelId::parse(id).ok_or_else(|| format!("line {}: {id:?} is not an id", n + 1))?;
        out.insert(id, tags.trim().to_owned());
    }
    Ok(out)
}

type Described = (BTreeMap<ModelId, Facts>, BTreeMap<ModelId, String>);

/// Every model's facts, and its picture when asked for, on `opts.threads` threads.
fn describe(
    store: &RetailDatStore,
    tally: &Tally,
    ids: &[ModelId],
    opts: &Options,
    thumbs_dir: &Path,
) -> Result<Described, String> {
    let next = AtomicUsize::new(0);
    let failed: Mutex<Option<String>> = Mutex::new(None);
    let results: Vec<Described> = std::thread::scope(|s| {
        let workers: Vec<_> = (0..opts.threads.max(1))
            .map(|_| {
                s.spawn(|| {
                    let mut models = Models::new(store);
                    let pictures = TextureStore::new(store);
                    // ORDER-OK: decode memos, only ever looked up.
                    let mut sizes: HashMap<DataId, Option<(u32, u32)>> = HashMap::new();
                    let mut materials: HashMap<DataId, Option<Material>> = HashMap::new();
                    let mut facts = BTreeMap::new();
                    let mut thumbs = BTreeMap::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        let Some(&id) = ids.get(i) else {
                            break;
                        };
                        let surfaces = tally
                            .piece_surfaces
                            .get(&id)
                            .map_or(&[][..], |(_, s)| s.as_slice());
                        // A texture's size is read once per thread.
                        let (f, mesh) = models.facts(id, surfaces, &mut |t| {
                            *sizes.entry(t).or_insert_with(|| {
                                pictures
                                    .resolve(t)
                                    .ok()
                                    .map(|(_, rs, _)| (rs.width, rs.height))
                            })
                        });
                        if opts.thumbnails && f.triangles > 0 {
                            let mut material = |s: Option<DataId>| -> Option<Material> {
                                let s = s?;
                                materials
                                    .entry(s)
                                    .or_insert_with(|| material_of(&mut models, &pictures, s))
                                    .clone()
                            };
                            let mut pic = thumb::draw(&mesh, &mut material, THUMBNAIL_SIZE, true);
                            if pic.is_blank() {
                                pic = thumb::draw(&mesh, &mut material, THUMBNAIL_SIZE, false);
                            }
                            match write_picture(&pic, id, thumbs_dir) {
                                Ok(Some(name)) => {
                                    thumbs.insert(id, name);
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    failed.lock().expect("not poisoned").get_or_insert(e);
                                }
                            }
                        }
                        facts.insert(id, f);
                    }
                    (facts, thumbs)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|w| w.join().expect("a drawing thread finishes"))
            .collect()
    });
    if let Some(e) = failed.into_inner().expect("not poisoned") {
        return Err(e);
    }
    let mut facts = BTreeMap::new();
    let mut thumbs = BTreeMap::new();
    for (f, t) in results {
        facts.extend(f);
        thumbs.extend(t);
    }
    Ok((facts, thumbs))
}

/// Writes `pic` as `thumbs/<id>.png` and answers that name; a blank picture is not written.
fn write_picture(pic: &thumb::Picture, id: ModelId, dir: &Path) -> Result<Option<String>, String> {
    if pic.is_blank() {
        return Ok(None);
    }
    let file = format!("{}.png", id.file_stem());
    let path = dir.join(&file);
    let bytes = pic.png().map_err(|e| format!("{}: {e}", path.display()))?;
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(format!("thumbs/{file}")))
}

/// What surface `s` draws with, decoded as the client decodes it: its picture (cut out where a
/// clip-mapped picture is transparent), or its flat colour; nothing for a surface mostly
/// see-through.
fn material_of(
    models: &mut Models<'_>,
    pictures: &TextureStore<'_>,
    s: DataId,
) -> Option<Material> {
    let surface = models.surface(s)?;
    // A translucent surface at least half see-through (a doorway, a window's glow) is left out.
    if surface.surface_type & surface_type::TRANSLUCENT != 0 && surface.translucency >= 0.5 {
        return Some(Material::Hidden);
    }
    if surface.orig_texture_id.is_none() {
        let c = surface.color_value.unwrap_or(0xFF96_9696);
        let [b, g, r, _] = c.to_le_bytes();
        return Some(Material::Colour([b, g, r, 255]));
    }
    let clip = surface.surface_type & surface_type::BASE1_CLIPMAP != 0;
    let image = if pictures.is_palettised(s).unwrap_or(false) {
        let d = pictures.texture_data_clipped(s, clip).ok()?;
        if d.format != TextureFormat::Bgra8 {
            return None;
        }
        let level = d.levels.into_iter().next()?;
        Image {
            width: d.width,
            height: d.height,
            bgra: level.as_chunks::<4>().0.to_vec(),
        }
    } else {
        let b = pictures.bgra8(s).ok()?;
        Image {
            width: b.width,
            height: b.height,
            bgra: b.pixels,
        }
    };
    Some(Material::Picture(Arc::new(image)))
}
