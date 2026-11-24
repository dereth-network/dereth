//! The AC2-in-AC1 world: Asheron's Call 2's landmass in Asheron's Call's formats, as a world
//! overlay over Asheron's Call's cell and portal files, built from a player's own installs.
//!
//! [`build`] reads acclient 00.00.11.6096's `client_cell_1.dat` and `client_portal.dat` and
//! Asheron's Call 2's end-of-retail `cell_1.dat` and `portal.dat` (refusing any other files, by
//! SHA-256), and writes the world `ac2-in-ac1` into the overlay folder:
//! - the cell overlay: every one of AC1's 65,025 landblocks as a dense record (3 vertex steps a
//!   24 m cell, 25 x 25 vertices 8 m apart), AC2's ground at its true size with AC2's origin on
//!   AC1's, heights lowered 70.95 m so AC2's sea is at 0 m; `lbi_exists` stays AC1's;
//! - the portal overlay: AC1's region with its grid (3 steps), road width (AC2's) and height table
//!   (fitted to the converted heights) set in place; and the map window's picture (`0x0600127D`),
//!   AC1's parchment frame with the converted land, drawn by Dereth's world map, in its marker
//!   box; AC2's ground pictures as the land slots' terrain textures; AC2's road masks as the
//!   region's road alpha maps, stretched to AC2's road width on the converted squares
//!   ([`roads`]); AC2's own cover (its trees, plants, rocks and grass) as the region's scene types
//!   and scene records, grown by AC1's scenery rules from samples of what AC2's own rules grow,
//!   every model AC2's, so the land names none of AC1's scenery ([`grown`], [`scenes`]); AC2's
//!   placed objects as AC1 statics ([`statics`]), set into the ground as AC2 sets them
//!   ([`buried`]), its ledges an AC1 body does not climb made climbable ([`ledges`], [`climb`]),
//!   its shells standing over rooms of their own made buildings you walk into ([`rooms`]);
//!   unless `--sky ac1`, AC2's sky as the region's sky, its models among the new records, with
//!   the world's sky tints beside the region (`0x13000001`), which color it by the hour
//!   ([`sky`]); and AC1's character-creation table (`0x0E000002`) with the world's one starting
//!   area, on dry land by Cragstone and named for the nearest of AC2's towns ([`start`]), or
//!   near the point a world's own `start.toml` gives (`--start FILE`, a world repository's
//!   `land/start.toml`);
//! - `world.toml`: the world's own rules for Empyrean (`dungeons_by_cells`, since AC2's land lies
//!   over AC1's dungeons; `own_placements`, since the world is built from AC2's content, not a
//!   world database's);
//! - unless `--names off`, `places.tsv`: AC2's towns and landmarks where AC2's map names them,
//!   which Dereth's map shows in place of AC1's notes ([`names`]);
//! - with `--vaults LIST`, AC2's dungeons the list names written under AC1 blocks ([`vaults`]),
//!   and with `--placements LIST`, `placements.tsv`: the things the world places itself (a
//!   dungeon's portals and the spots they lead to), Empyrean's own placements table, each row's
//!   cell checked to be one the world has.
//!
//! The overlay's iterations are numbered from its content ([`iteration`]): the ledger keeps each
//! content built with its generation, so a world rebuilt with changes is numbered past the one
//! players already hold, and the same content always takes the same numbers.
//!
//! Beside the overlay folder, the build's reports: `README.txt` (how it was made, and its inputs'
//! and outputs' SHA-256), `stats.txt`, `dungeons.tsv` (AC1's dungeon blocks and what lies over
//! them now), `ac1-removed.tsv` (each block's AC1 buildings, their rooms, its statics and its
//! house restrictions left off the converted block), `worst-loss.tsv` (the AC2 vertices the
//! conversion moves most), `kinds.tsv` (each AC2 kind's AC1 slot and variant), `map-picture.png`
//! (the map window's new picture), `ground.tsv` (each terrain slot's AC2 picture),
//! `road-maps.tsv` (each road alpha map's AC2 mask), `cover-kinds.tsv` and `cover-setups.tsv`
//! (AC2's cover kinds and the setups their scenes place), `statics-showing.tsv` (the placed
//! statics left showing above the converted ground where AC2 sets them into its own: past the cap
//! on raising the land, or on a square with a water corner), `statics-surfaces.tsv` (each placed
//! model's material not drawn with its own picture: drawn in a plain color, from a smaller level,
//! or not at all), `statics-made.tsv` (each placed model with weights or a behavior table, and how
//! it was made: still, at rest, or moving), `statics-left.tsv` (each object whose placements are
//! left out for what it is, and why), `statics-effects.tsv` (each object that plays effects,
//! whether it draws anything of its own, and the AC1 emitters and lights they became),
//! `ledges.tsv` (each placement with ledges AC1's bodies do not
//! climb, and the land raised and ramps made for them), `ramps.tsv` (each ramp, and the walk
//! along its foot before and after) and `sky.tsv` (each AC2 sky object of each day, and what it
//! became). The same inputs give the same bytes, the reports too.
//!
//! Everything it writes is made from the player's own installs and stays with them.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read as _;
use std::path::{Path, PathBuf};

mod art;
mod buried;
mod cleared;
mod clearing;
mod climb;
mod dense;
mod effects;
mod ground;
mod grown;
mod iteration;
mod kinds;
mod ledges;
mod map;
mod names;
mod passage;
mod placeholder;
mod polys;
mod region;
mod roads;
mod rooms;
mod scenes;
mod sky;
mod start;
mod statics;
mod steep;
mod table;
mod textures;
mod vaults;

use crate::ac2_world::dense::{DenseWorld, Grid};
use crate::ac2_world::ground::Ac2Ground;
use crate::ac2_world::scenes::Key;
use auberean_assets::Context;
use auberean_dat::locate::DatName;
use auberean_world::records::WorldRecords;
use dereth_assets::{CellLandblock, Decode, DenseLand, LandblockInfo, Region, RenderSurface};
use dereth_dat::overlay::{container_name, OverlayDir, OverlayWriter};
use dereth_dat::{DatFile, ModernDat};
use dereth_primitives::DataId;
use sha2::{Digest, Sha256};

/// The world key the overlay is written for.
const WORLD_KEY: &str = "ac2-in-ac1";
/// The region record.
const REGION_ID: u32 = 0x1300_0000;
/// The map window's picture.
const MAP_ID: u32 = 0x0600_127D;
/// The character-creation table.
const CHARGEN_ID: u32 = 0x0E00_0002;
/// The overlay's deletions record (its id in the overlay's own range), for the content digest.
const TOMBSTONES_ID: u32 = 0xFFFF_0002;
/// The map picture's pixel format: three bytes a pixel, blue, green, red.
const PFID_R8G8B8: u32 = 20;
/// Vertex steps a 24 m cell.
const STEPS: usize = 3;
/// The greatest rounding error the fitted height table may make, in meters.
const MAX_ROUNDING: f64 = 2.0;
/// How far round a placed static AC1's scenery is kept off, meters.
const SCENERY_MARGIN: f64 = 4.0;

/// The files read, and their SHA-256: acclient 00.00.11.6096's, and Asheron's Call 2's at the end
/// of retail.
const INPUTS: [(Side, &str, &str); 4] = [
    (
        Side::Ac1,
        "client_cell_1.dat",
        "6db0abf00fbceed62c3f1ee842ee7c1f423d732bed77a5b7c102ee89a52ab99e",
    ),
    (
        Side::Ac1,
        "client_portal.dat",
        "dc6e500ba22e6b186db7171e3f3345238b6444c85d798adc85e550973b8d12e4",
    ),
    (
        Side::Ac2,
        "cell_1.dat",
        "e855b98216f1af06da4097c9196f221c36e62af565922cf88ffd390fe396a258",
    ),
    (
        Side::Ac2,
        "portal.dat",
        "f207b3db502efaff78d11893b2cdda33f42ff6da8fe1bbb795f6ea9a3b2d5772",
    ),
];

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Ac1,
    Ac2,
}

/// What a world build is asked to do.
#[derive(Debug, Clone)]
pub struct Args {
    /// The Asheron's Call install (acclient 00.00.11.6096's files).
    pub ac1: PathBuf,
    /// The Asheron's Call 2 install (its end-of-retail files).
    pub ac2: PathBuf,
    /// The overlay folder written.
    pub out: PathBuf,
    /// The height table: `fitted` to the converted heights, or `ac2`'s own.
    pub table: String,
    /// The most levels of detail a static's models are given (one unless asked).
    pub levels: usize,
    /// Whether the overlay folder names AC2's towns and landmarks (`places.tsv`); on unless asked.
    pub names: bool,
    /// AC2 dungeons written under AC1 blocks (`--vaults`, [`vaults`]).
    pub vaults: Option<PathBuf>,
    /// The world's own placements (`--placements`), written beside the rules.
    pub placements: Option<PathBuf>,
    /// Where new characters start (`--start`, a `start.toml`): a world's own, else the tool's
    /// ([`start::START_TOML`]).
    pub start: Option<PathBuf>,
    /// Whether AC2's frills (its grass and small plants, drawn only near the viewer in AC2) are
    /// grown with its cover ([`grown`]); on unless asked.
    pub frills: bool,
    /// The sky: AC2's (`--sky ac2`, the default), every day or the `--sky-day`th alone, with its
    /// tints ([`sky`]); or AC1's own (`--sky ac1`).
    pub sky: bool,
    pub sky_day: Option<usize>,
    /// The ledger of the contents written so far, each with its generation ([`iteration`]).
    pub ledger: PathBuf,
    /// Whether to build from installs whose files are not the expected ones (`--any-install`),
    /// which may give a world that matches no one else's.
    pub any_install: bool,
    /// The command line, for the build's notes.
    pub command: String,
}

/// The usage line of `dereth-art ac2-world`.
pub const USAGE: &str = "dereth-art ac2-world --ac1 DIR --ac2 DIR --out DIR [--table fitted|ac2] [--levels 1|2|3] [--names on|off] [--frills on|off] [--sky ac2|ac1] [--sky-day 1-7] [--vaults FILE --placements FILE] [--start FILE] [--ledger FILE] [--any-install]";

impl Args {
    /// The arguments after `ac2-world`.
    ///
    /// # Errors
    /// An argument that is not one of [`USAGE`]'s, or a required one missing.
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let (mut ac1, mut ac2, mut out, mut tab) = (None, None, None, "fitted".to_owned());
        let mut levels = 1;
        let mut names = true;
        let mut vaults = None;
        let mut placements = None;
        let mut start = None;
        let mut frills = true;
        let (mut sky, mut sky_day) = (true, None);
        let mut ledger = None;
        let mut any_install = false;
        let mut a = args.iter();
        while let Some(k) = a.next() {
            if k == "--any-install" {
                any_install = true;
                continue;
            }
            let v = a
                .next()
                .ok_or_else(|| format!("{k} needs a value"))?
                .clone();
            match k.as_str() {
                "--ac1" => ac1 = Some(PathBuf::from(v)),
                "--ac2" => ac2 = Some(PathBuf::from(v)),
                "--out" => out = Some(PathBuf::from(v)),
                "--table" if v == "fitted" || v == "ac2" => tab = v,
                "--names" if v == "on" || v == "off" => names = v == "on",
                "--frills" if v == "on" || v == "off" => frills = v == "on",
                "--sky" if v == "ac2" || v == "ac1" => sky = v == "ac2",
                "--sky-day" => {
                    sky_day = Some(
                        v.parse::<usize>()
                            .ok()
                            .filter(|n| (1..=7).contains(n))
                            .ok_or_else(|| format!("--sky-day {v}: 1 to 7"))?,
                    );
                }
                "--vaults" => vaults = Some(PathBuf::from(v)),
                "--placements" => placements = Some(PathBuf::from(v)),
                "--start" => start = Some(PathBuf::from(v)),
                "--ledger" => ledger = Some(PathBuf::from(v)),
                "--levels" => {
                    levels = v
                        .parse::<usize>()
                        .ok()
                        .filter(|n| (1..=statics::MAX_LEVELS).contains(n))
                        .ok_or_else(|| format!("--levels {v}: 1 to {}", statics::MAX_LEVELS))?;
                }
                _ => return Err(format!("unknown argument {k} {v}")),
            }
        }
        let out: PathBuf = out.ok_or("--out")?;
        let ledger = ledger.unwrap_or_else(|| beside_of(&out).join("generations.tsv"));
        Ok(Self {
            ac1: ac1.ok_or("--ac1")?,
            ac2: ac2.ok_or("--ac2")?,
            out,
            table: tab,
            levels,
            names,
            vaults,
            placements,
            start,
            frills,
            sky,
            sky_day,
            ledger,
            any_install,
            command: std::iter::once("dereth-art ac2-world".to_owned())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>()
                .join(" "),
        })
    }
}

fn sha256(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 8 << 20];
    loop {
        let n = f
            .read(&mut buf)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Every input's SHA-256, refusing a file that is not the expected one.
fn check_inputs(args: &Args) -> Result<Vec<String>, String> {
    let lines = std::thread::scope(|s| {
        let jobs: Vec<_> = INPUTS
            .iter()
            .map(|&(side, name, want)| {
                let dir = if side == Side::Ac1 {
                    &args.ac1
                } else {
                    &args.ac2
                };
                let path = dir.join(name);
                let any = args.any_install;
                s.spawn(move || -> Result<String, String> {
                    let got = sha256(&path)?;
                    if got != want && any {
                        eprintln!(
                            "{} is not the expected file (SHA-256 {got}, expected {want}): built from it anyway, so this world may not match anyone else's",
                            path.display()
                        );
                        return Ok(format!(
                            "{got}  {} (not the expected file, {want})",
                            path.display()
                        ));
                    }
                    if got != want {
                        return Err(format!(
                            "{} is not the expected file (SHA-256 {got}, expected {want}); --any-install builds from it anyway",
                            path.display()
                        ));
                    }
                    Ok(format!("{got}  {}", path.display()))
                })
            })
            .collect();
        jobs.into_iter()
            .map(|j| j.join().expect("a hashing thread"))
            .collect::<Result<Vec<_>, _>>()
    })?;
    Ok(lines)
}

/// AC2's ground, world-wide, and its height table and road width.
fn read_ac2(
    dir: &Path,
) -> Result<
    (
        Ac2Ground,
        [f32; 256],
        f32,
        Vec<crate::ac2_world::steep::Steep>,
    ),
    String,
> {
    read_ac2_area(dir, (0, 0), 255)
}

/// AC2's ground over the `blocks` x `blocks` blocks from AC2 block `origin` (the world's own
/// blocks numbered from it), and its height table and road width.
fn read_ac2_area(
    dir: &Path,
    origin: (usize, usize),
    blocks: usize,
) -> Result<
    (
        Ac2Ground,
        [f32; 256],
        f32,
        Vec<crate::ac2_world::steep::Steep>,
    ),
    String,
> {
    let open = |n: DatName| {
        let p = n.in_dir(dir);
        auberean_dat::DatFile::open(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let portal = open(DatName::Portal)?;
    let cell = open(DatName::Cell(1))?;
    let ctx = Context::from_portal(&portal).map_err(|e| format!("AC2 property list: {e:?}"))?;
    let w = WorldRecords::new(portal, cell, None, ctx);
    let r = w.region().map_err(|e| format!("AC2 region: {e:?}"))?;
    let land = &r.land;
    let grid = land.grid();
    let squares = grid.cells_per_side * grid.vertices_per_cell;
    let square_len = f64::from(grid.cell_length) / grid.vertices_per_cell as f64;
    let mut g = Ac2Ground::new(blocks, squares, square_len, land.spec.split);
    let water = r.water.as_ref().ok_or("AC2's region has no water")?;
    let n = (squares + 1) * (squares + 1);
    let mut held = 0;
    for bx in 0..blocks {
        for by in 0..blocks {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let (x, y) = ((origin.0 + bx) as i32, (origin.1 + by) as i32);
            g.plane[bx * blocks + by] =
                water.block(land, x, y, f32::NEG_INFINITY).map(|b| b.height);
            let Some(lb) = w.landblock(x, y) else {
                continue;
            };
            if lb.heights.len() != n || lb.vertices.len() != n {
                return Err(format!("AC2 block {bx:02X}{by:02X} is not {n} vertices"));
            }
            let h: Vec<f32> = lb
                .heights
                .iter()
                .map(|&b| land.heights[usize::from(b)])
                .collect();
            let byte = |f: auberean_world::land::Field| -> Vec<u8> {
                lb.vertices
                    .iter()
                    .map(|&v| u8::try_from(f.of(v)).unwrap_or(u8::MAX))
                    .collect()
            };
            g.put_block(
                bx,
                by,
                &h,
                &byte(land.word.terrain),
                &byte(land.word.scene),
                &byte(land.word.road),
            );
            held += 1;
        }
    }
    eprintln!("AC2: {held} blocks, {squares} squares of {square_len} m a block");
    // Each vertex's material group, by its slope as AC2 works it out, and the steep variants.
    let (by_kind, steep) = crate::ac2_world::steep::allocate(&r.terrain, &r.surface);
    let side = g.side();
    let groups = crate::ac2_world::dense::par_map(side, |gx| {
        (0..side)
            .map(|gy| {
                let k = gx * side + gy;
                if g.height[k].is_nan() {
                    return 0;
                }
                #[allow(clippy::cast_possible_wrap)]
                let pitch = g.pitch(gx as i64, gy as i64);
                crate::ac2_world::steep::surface_of(&r.terrain, &r.surface, u32::from(g.kind[k]))
                    .and_then(|s| crate::ac2_world::steep::group_at(s, pitch))
                    .and_then(|gi| u8::try_from(gi).ok())
                    .unwrap_or(0)
            })
            .collect::<Vec<u8>>()
    });
    g.group = groups.into_iter().flatten().collect();
    g.steep = by_kind;
    for v in &steep {
        eprintln!(
            "steep: slot {} variant {} draws AC2 material {:08X} for kinds and groups {:?}",
            v.slot, v.variant, v.material, v.from
        );
    }
    Ok((g, land.heights, land.road_width, steep))
}

/// Percentiles and mean of a set of errors.
fn summary(mut v: Vec<f32>) -> String {
    if v.is_empty() {
        return "none".into();
    }
    v.sort_by(f32::total_cmp);
    let n = v.len();
    let mean = v.iter().map(|&x| f64::from(x)).sum::<f64>() / n as f64;
    // LINT-OK: an index into the list, from a fraction of its length.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let p = |q: f64| v[((n as f64 - 1.0) * q).round() as usize];
    let over = |t: f32| v.iter().filter(|&&x| x > t).count();
    format!(
        "n {n}, mean {mean:.3} m, p50 {:.3}, p99 {:.3}, max {:.3}; over 1 m {}, over 2 m {}, over 5 m {}",
        p(0.5),
        p(0.99),
        v[n - 1],
        over(1.0),
        over(2.0),
        over(5.0)
    )
}

/// The rounding error of `t` over the converted land vertices.
fn rounding_errors(w: &DenseWorld, t: &[f32; 256]) -> Vec<f32> {
    w.vertex
        .iter()
        .filter(|v| !v.water)
        .map(|v| {
            let q = t[usize::from(table::quantize(t, f64::from(v.exact)))];
            (q - v.exact).abs()
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
/// Build the world: read both installs and write the overlay folder and the reports beside it.
///
/// # Errors
/// An input that is not the expected file, or one that will not read or write.
pub fn build(args: &Args) -> Result<(), String> {
    let t0 = std::time::Instant::now();
    eprintln!("checking the inputs' SHA-256");
    let fingerprints = check_inputs(args)?;
    eprintln!("reading AC2");
    let (mut g, ac2_table, ac2_road, steep) = read_ac2(&args.ac2)?;
    // AC2's placeholder land (the copied islands in the far north, the flat specks) left out as
    // sea; the ground as it was kept to show what that changed.
    let whole = g.clone();
    let dropped = placeholder::drop_placeholder_land(&mut g);
    for p in &dropped {
        eprintln!(
            "left out ({:?}): AC2 blocks {:02X}{:02X}-{:02X}{:02X}, {} blocks",
            p.why, p.south_west.0, p.south_west.1, p.north_east.0, p.north_east.1, p.blocks
        );
    }

    let cell_path = args.ac1.join(ModernDat::Cell.file_name());
    let portal_path = args.ac1.join(ModernDat::Portal.file_name());
    let cell = DatFile::open(&cell_path).map_err(|e| format!("{}: {e}", cell_path.display()))?;
    let portal =
        DatFile::open(&portal_path).map_err(|e| format!("{}: {e}", portal_path.display()))?;
    let region_bytes = portal
        .read(DataId(REGION_ID))
        .map_err(|e| format!("AC1 region: {e}"))?;
    let ac1_region = Region::decode_payload(DataId(REGION_ID), &region_bytes)
        .map_err(|e| format!("AC1 region: {e:?}"))?;
    let ld = &ac1_region.land_defs;
    if ld.num_block_length != 255
        || ld.num_block_width != 255
        || ld.square_length != 24.0
        || ld.lblock_length != 8
        || ld.vertex_per_cell != 1
    {
        return Err(format!("AC1's land definitions are not retail's: {ld:?}"));
    }
    let grid = Grid {
        blocks: 255,
        cells: 8,
        cell_len: f64::from(ld.square_length),
        steps: STEPS,
    };

    eprintln!("sampling {} x {} vertices", grid.side(), grid.side());
    let land_before = land_blocks(&DenseWorld::sample(&whole, grid));
    drop(whole);
    let mut w = DenseWorld::sample(&g, grid);
    let land_after = land_blocks(&w);
    // Every AC1 block that lost its land lies over a stretch left out, and no block gained land.
    let gained = land_after
        .iter()
        .zip(&land_before)
        .filter(|&(&a, &b)| a && !b)
        .count();
    let lost: Vec<usize> = (0..land_before.len())
        .filter(|&k| land_before[k] && !land_after[k])
        .collect();
    let over_dropped = |k: usize| {
        let (bx, by) = (k / 255, k % 255);
        let (x0, y0) = (bx as f64 * 192.0, by as f64 * 192.0);
        dropped.iter().any(|p| {
            let len = g.block_len();
            let (sx, sy) = (p.south_west.0 as f64 * len, p.south_west.1 as f64 * len);
            let (ex, ey) = (
                (p.north_east.0 + 1) as f64 * len,
                (p.north_east.1 + 1) as f64 * len,
            );
            x0 < ex && x0 + 192.0 > sx && y0 < ey && y0 + 192.0 > sy
        })
    };
    if gained != 0 || !lost.iter().all(|&k| over_dropped(k)) {
        return Err(format!(
            "leaving out the placeholder land changed other land: {gained} blocks gained, {} lost elsewhere",
            lost.iter().filter(|&&k| !over_dropped(k)).count()
        ));
    }
    let heights: Vec<f32> = w
        .vertex
        .iter()
        .filter(|v| !v.water)
        .map(|v| v.exact)
        .collect();
    let (fitted, fit) = table::fitted(&heights, MAX_ROUNDING);
    eprintln!("fitted table: mean {:.3} m, max {:.3} m", fit.mean, fit.max);
    // The trade between the mean and the greatest error, for the report.
    let mut ladder = String::new();
    for m in [1.0, 1.5, 2.0, 2.5, 5.0, 100.0] {
        let (_, r) = table::fitted(&heights, m);
        let _ = write!(ladder, " {m} m: mean {:.3}, max {:.3};", r.mean, r.max);
    }
    let lowered = table::lowered(&ac2_table);
    if !table::is_valid(&fitted) {
        return Err("the fitted table is not ascending".into());
    }
    let rounding_fitted = summary(rounding_errors(&w, &fitted));
    let rounding_lowered = summary(rounding_errors(&w, &lowered));
    let chosen = if args.table == "ac2" { lowered } else { fitted };
    if !chosen.windows(2).all(|p| p[0] <= p[1]) || chosen[0] != 0.0 {
        return Err("the chosen table is not ascending from 0".into());
    }
    eprintln!("rounding and cutting");
    w.finish(&g, chosen);

    // AC2's placed statics, made as AC1 models and placed at AC2's places, standing on the
    // converted ground as they stood on AC2's.
    eprintln!("placing AC2's statics");
    let ac2_art = crate::ac2_look::source::Ac2::open(&args.ac2)?;
    let (mut ids, mut pictures, mut art_records) = (
        art::Ids::default(),
        art::Pictures::default(),
        art::Records::default(),
    );
    let on_dropped = |bx: usize, by: usize| {
        dropped.iter().any(|p| {
            (p.south_west.0..=p.north_east.0).contains(&bx)
                && (p.south_west.1..=p.north_east.1).contains(&by)
        })
    };
    // AC2's buildings with rooms of their own: their shells are made AC1 buildings below, not
    // placed as statics.
    let ac2_world = statics::world_records(&args.ac2)?;
    let rooms_found = rooms::find(&ac2_world);
    let shells_asked = rooms_found.shells();
    let mut statics = {
        let (step, wside) = (w.grid.spacing(), w.grid.side());
        // LINT-OK: square indices of a world a few thousand squares wide.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let under = |x: f64, y: f64| -> Option<statics::Under> {
            let ac2_ground = g.ground(x, y)?;
            if x < 0.0 || y < 0.0 {
                return None;
            }
            let (i, j) = ((x / step).floor() as usize, (y / step).floor() as usize);
            if i + 1 >= wside || j + 1 >= wside {
                return None;
            }
            let wet = [(i, j), (i + 1, j), (i, j + 1), (i + 1, j + 1)]
                .iter()
                .any(|&(a, b)| w.vertex[a * wside + b].water);
            Some((ac2_ground, w.height_at(x, y, true), wet, (i, j)))
        };
        statics::place_statics(
            &args.ac2,
            &ac2_art,
            &on_dropped,
            &under,
            &shells_asked,
            &mut pictures,
            &mut ids,
            &mut art_records,
            args.levels,
        )?
    };
    eprintln!(
        "statics: {} placed, {} models, left out {:?}",
        statics.placed, statics.models, statics.left_out
    );
    // AC2's buildings with rooms of their own, made AC1 buildings, by the AC1 block each stands in.
    let mut rooms_passed = rooms_found.passed.clone();
    let mut ac2_buildings: std::collections::BTreeMap<(usize, usize), Vec<rooms::Building>> =
        std::collections::BTreeMap::new();
    for b in rooms::make(
        &rooms_found,
        &statics.shells,
        &ac2_world,
        &ac2_art,
        &mut pictures,
        &mut ids,
        &mut art_records,
        &mut rooms_passed,
        &mut statics.by_block,
    )? {
        ac2_buildings.entry(b.block).or_default().push(b);
    }
    let mut vault_cells: std::collections::BTreeMap<(usize, usize), Vec<vaults::Made>> =
        std::collections::BTreeMap::new();
    if let Some(p) = &args.vaults {
        let list = vaults::parse_list(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )?;
        for v in &list {
            let m = vaults::make(
                v,
                &ac2_world,
                &ac2_art,
                &mut pictures,
                &mut ids,
                &mut art_records,
            )?;
            eprintln!(
                "AC2 dungeon {:02X}{:02X}: {} cells made, {} portals left closed, {} lights placed, {} left out",
                v.ac2.0,
                v.ac2.1,
                m.cells.len(),
                m.closed,
                m.lights,
                m.lights_left
            );
            vault_cells.entry(m.block).or_default().push(m);
        }
    }
    eprintln!(
        "AC2's buildings with rooms: {} made, {} rooms; passed over {rooms_passed:?}",
        ac2_buildings.values().map(Vec::len).sum::<usize>(),
        ac2_buildings
            .values()
            .flatten()
            .map(|b| b.rooms.len())
            .sum::<usize>()
    );
    // The land raised to cover what AC2 sets into its ground and the converted ground leaves
    // showing, on dry squares and within the cap.
    let wants =
        crate::ac2_world::buried::wants(&statics.covers, w.grid.side(), &w.height, &w.table);
    let raised = w.raise_to(&g, &wants);
    let past_cap = statics.covers.iter().filter(|c| c.past_cap > 0.0).count();
    let on_water = statics.covers.iter().filter(|c| c.wet > 0.0).count();
    let covered = statics.covers.iter().filter(|c| !c.dry.is_empty()).count();
    let buried_line = format!(
        "statics set into AC2's ground: {} placed by where they are set in ({} more than 0.5 m from where their origin put them); land raised under {covered} of them, {raised} vertices; left showing more than {} m: {past_cap} past the {} m cap, {on_water} on squares with a water corner (statics-showing.tsv)",
        statics.set_in,
        statics.moved,
        crate::ac2_world::buried::TOLERANCE,
        crate::ac2_world::buried::CAP,
    );
    eprintln!("{buried_line}");
    // AC2's ledges AC1's bodies do not climb: the land raised where they drop to the terrain, a
    // collision-only ramp where they drop to another static.
    eprintln!("making AC2's ledges climbable");
    let check = climb::Check {
        ac1: &args.ac1,
        cell: &cell,
        portal: &portal,
        region: (
            REGION_ID,
            region::patch(
                &region_bytes,
                u32::try_from(STEPS).unwrap_or(3),
                ac2_road,
                &w.table,
            )?,
        ),
        dir: beside_of(&args.out).join("work").join("ledge-walks"),
        world_key: WORLD_KEY,
        steps: STEPS,
    };
    // The steps of the shells with AC2's rooms are ramped where AC1 does not climb them.
    let steps_of = rooms_found.shells();
    let climb = climb::make(
        &g,
        &mut w,
        &mut statics,
        &steps_of,
        &mut ids,
        &mut art_records,
        &check,
    )?;
    eprintln!("{}", climb.line);
    let mut busiest: Vec<(usize, (usize, usize))> = statics
        .by_block
        .iter()
        .map(|(b, v)| (v.len(), *b))
        .collect();
    busiest.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    // AC2's own cover, grown by AC1's scenery rules from scenes made of AC2's samples
    // ([`grown`]): every vertex's scene index names, among its terrain type's, the cover kind AC2
    // grows at the AC2 vertex nearest it. A scene index naming no kind, and the last of every type,
    // name an empty scene description.
    let empty_scene = ac1_region
        .scene_info
        .iter()
        .flatten()
        .position(|d| d.scenes.is_empty())
        .and_then(|k| i32::try_from(k).ok())
        .ok_or("AC1's region has no empty scene description")?;
    eprintln!("growing AC2's cover over AC2's world");
    let ac2_region = ac2_world
        .region()
        .map_err(|e| format!("AC2 region: {e:?}"))?;
    let census = grown::census(&ac2_world, &ac2_region, &g, &on_dropped);
    eprintln!(
        "AC2's cover: {} objects and {} frills on {} blocks ({} at an uneven scale), {} kinds",
        census.total[0],
        census.total[1],
        census.blocks,
        census.uneven,
        census.vertices.len()
    );
    let kind_table = grown::KindTable::new(&ac2_region);
    let wside = grid.side();
    let spacing = grid.spacing();
    let key_of = |i: usize, j: usize| {
        g.nearest_vertex(i as f64 * spacing, j as f64 * spacing)
            .and_then(|v| kind_table.key(&g, v))
    };
    let mut corner_counts: BTreeMap<u16, BTreeMap<Key, usize>> = BTreeMap::new();
    for row in crate::ac2_world::dense::par_map(wside.div_ceil(STEPS), |ci| {
        let i = ci * STEPS;
        let mut c: BTreeMap<(u16, Key), usize> = BTreeMap::new();
        for j in (0..wside).step_by(STEPS) {
            if let Some(k) = key_of(i, j) {
                *c.entry(((w.vertex[i * wside + j].word >> 2) & 0x1F, k))
                    .or_default() += 1;
            }
        }
        c
    }) {
        for ((slot, k), n) in row {
            *corner_counts.entry(slot).or_default().entry(k).or_default() += n;
        }
    }
    let slots = crate::ac2_world::scenes::Slots::from_counts(&corner_counts);
    let indices = crate::ac2_world::dense::par_map(wside, |i| {
        (0..wside)
            .map(|j| slots.index((w.vertex[i * wside + j].word >> 2) & 0x1F, key_of(i, j)))
            .collect::<Vec<u16>>()
    });
    let mut growing_corners = 0usize;
    for (i, row) in indices.into_iter().enumerate() {
        for (j, index) in row.into_iter().enumerate() {
            let v = &mut w.vertex[i * wside + j];
            v.word = (v.word & 0x07FF) | (index << 11);
            growing_corners += usize::from(
                i % STEPS == 0 && j % STEPS == 0 && index != crate::ac2_world::clearing::NO_SCENE,
            );
        }
    }
    let kept_keys = slots.all_keys();
    eprintln!(
        "AC2's cover: {} kinds kept on {} terrain types, {growing_corners} land cell corners grow one; folded into a kept kind: {:?}",
        kept_keys.len(),
        slots.keys.len(),
        slots.folded
    );
    eprintln!("sampling AC2's cover");
    let (samples, middles) = grown::tiles(&ac2_world, &ac2_region, &g, &census, &kept_keys);
    let mut cover_samples_tsv = String::from(
        "ac2_scenery_kind	under_water	sample	x_m	y_m	ac1_block	objects	frills
",
    );
    for (k, list) in &middles {
        for (n, &(x, y)) in list.iter().enumerate() {
            let t = &samples[k][n];
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let _ = writeln!(
                cover_samples_tsv,
                "{:08X}	{}	{n}	{x:.1}	{y:.1}	{:02X}{:02X}	{}	{}",
                k.0,
                k.1,
                (x / grid.cell_len / grid.cells as f64) as usize,
                (y / grid.cell_len / grid.cells as f64) as usize,
                t.iter().filter(|o| !o.frill).count(),
                t.iter().filter(|o| o.frill).count()
            );
        }
    }
    let made = grown::make_scenes(
        &samples,
        &census,
        &ac2_world,
        &ac2_art,
        &mut pictures,
        &mut ids,
        &mut art_records,
        args.levels,
        args.frills,
    )?;
    let cover_line = format!(
        "AC2's cover: {} objects and {} frills grown by AC2 on {} blocks; {} kinds kept on {} terrain types ({growing_corners} land cell corners grow one); {} scenes of up to {} samples a kind, holding {} objects and {} frills ({} setups of {} models); made as built for their idle bends them: {:08X?}; frills' blended pictures cut out: {}; left out of the samples {:?}; frills {}",
        census.total[0],
        census.total[1],
        census.blocks,
        kept_keys.len(),
        slots.keys.len(),
        made.scenes,
        grown::TILES_PER_KEY,
        made.objects[0],
        made.objects[1],
        made.setups,
        made.models,
        made.bent,
        made.cut_out,
        made.left_out,
        if args.frills { "on" } else { "off" },
    );
    eprintln!("{cover_line}");
    // The region's scene types: AC1's, then one for each kept kind (its samples' scenes, with the
    // ambient sounds of the terrain type it is commonest on), and each terrain type's list.
    let mut scene_info = ac1_region.scene_info.clone().unwrap_or_default();
    let mut desc_of: BTreeMap<Key, i32> = BTreeMap::new();
    for key in &kept_keys {
        let Some(scenes) = made.scenes_of.get(key).filter(|s| !s.is_empty()) else {
            continue;
        };
        let home = corner_counts
            .iter()
            .filter_map(|(slot, c)| c.get(key).map(|n| (*n, *slot)))
            .max()
            .map_or(0, |(_, slot)| slot);
        let stb_index = ac1_region
            .terrain_types
            .get(usize::from(home))
            .and_then(|t| t.scene_types.first())
            .and_then(|&d| usize::try_from(d).ok())
            .and_then(|d| scene_info.get(d))
            .map_or(0, |d| d.stb_index);
        desc_of.insert(*key, i32::try_from(scene_info.len()).unwrap_or(empty_scene));
        scene_info.push(dereth_assets::region::SceneDesc {
            stb_index,
            scenes: scenes.clone(),
        });
    }
    let scene_lists = slots.lists(
        ac1_region.terrain_types.len(),
        &|k| desc_of.get(&k).copied().unwrap_or(empty_scene),
        empty_scene,
    );
    let cover_kinds_tsv = grown::kinds_tsv(&census, &samples);
    // The statics keep it off: every cell under a static, or within the margin, names nothing.
    let cells_side = grid.blocks * grid.cells;
    let mut cleared = std::collections::BTreeSet::new();
    for f in &statics.footprints {
        for c in
            crate::ac2_world::clearing::cells_under(*f, SCENERY_MARGIN, grid.cell_len, cells_side)
        {
            cleared.insert(c);
        }
    }
    for &(cx, cy) in &cleared {
        let k = cx * STEPS * wside + cy * STEPS;
        w.vertex[k].word = crate::ac2_world::clearing::without_scenery(w.vertex[k].word);
    }
    eprintln!(
        "scenery cleared from {} cells under the statics",
        cleared.len()
    );
    let mut statics_blocks = String::from("ac1_block\tstatics\n");
    for (n, (bx, by)) in &busiest {
        let _ = writeln!(statics_blocks, "{bx:02X}{by:02X}\t{n}");
    }

    // The loss against AC2's own land vertices: the converted ground at each, exact and rounded.
    eprintln!("measuring the loss");
    let side2 = g.side();
    let rows = par_rows(side2, |gx| {
        let mut out = Vec::new();
        for gy in 0..side2 {
            if !g.is_land(gx, gy) {
                continue;
            }
            let (x, y) = (gx as f64 * g.square_len, gy as f64 * g.square_len);
            let want = table::lower(f64::from(g.height[gx * side2 + gy]));
            #[allow(clippy::cast_possible_truncation)]
            let exact = (w.height_at(x, y, false) - want).abs() as f32;
            #[allow(clippy::cast_possible_truncation)]
            let rounded = (w.height_at(x, y, true) - want).abs() as f32;
            // The steepest rise to a neighbor: a cliff or a spike 10 m wide.
            let h0 = g.height[gx * side2 + gy];
            let mut relief = 0f32;
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (gx as i64 + dx, gy as i64 + dy);
                if a < 0 || b < 0 || a >= side2 as i64 || b >= side2 as i64 {
                    continue;
                }
                // LINT-OK: both checked non-negative and inside the world just above.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let h = g.height[a as usize * side2 + b as usize];
                if !h.is_nan() {
                    relief = relief.max((h - h0).abs());
                }
            }
            out.push((exact, rounded, gx, gy, relief));
        }
        out
    });
    let loss: Vec<(f32, f32, usize, usize, f32)> = rows.into_iter().flatten().collect();
    let big: Vec<&(f32, f32, usize, usize, f32)> = loss.iter().filter(|l| l.1 > 5.0).collect();
    let big_steep = big.iter().filter(|l| l.4 > 10.0).count();
    let loss_exact = summary(loss.iter().map(|l| l.0).collect());
    let loss_rounded = summary(loss.iter().map(|l| l.1).collect());
    let mut worst = loss.clone();
    worst.sort_by(|a, b| b.1.total_cmp(&a.1).then((a.2, a.3).cmp(&(b.2, b.3))));
    let mut worst_tsv = String::from(
        "x_m\ty_m\tac2_block\tac1_block\tac2_height_lowered\tconverted\terror\tsteepest_neighbor_rise\n",
    );
    for &(_, e, gx, gy, relief) in worst.iter().take(200) {
        let (x, y) = (gx as f64 * g.square_len, gy as f64 * g.square_len);
        // LINT-OK: a point of the world, a few tens of thousands of meters, to its block's index.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (lbx, lby) = ((x / 192.0) as usize, (y / 192.0) as usize);
        let want = table::lower(f64::from(g.height[gx * side2 + gy]));
        let _ = writeln!(
            worst_tsv,
            "{x}\t{y}\t{:02X}{:02X}\t{:02X}{:02X}\t{want:.2}\t{:.2}\t{e:.2}\t{relief:.2}",
            gx / g.squares,
            gy / g.squares,
            lbx,
            lby,
            w.height_at(x, y, true)
        );
    }

    // Per AC1 block.
    eprintln!("reading AC1's landblocks");
    let s = grid.block_squares();
    let side = grid.side();
    let mut land_blocks = 0;
    let mut part_water = 0;
    let mut all_water = 0;
    let mut edge_blocks = 0;
    let mut basin_blocks = 0;
    let water_vertices = w.vertex.iter().filter(|v| v.water).count();
    let water_vertices_in_ac2 = w.vertex.iter().filter(|v| v.water && !v.no_ground).count();
    let land_vertices = w.vertex.iter().filter(|v| !v.water).count();
    let floored = w.vertex.iter().filter(|v| v.floored).count();
    let edge_vertices = w.edge_rule.iter().filter(|&&e| e).count();
    let mut dungeons = String::from(
        "block\tenv_cells\tconverted_land_vertices\tconverted_water_vertices\tnonzero_heights\tmax_height_m\tstill_all_zero\n",
    );
    let (mut dungeon_count, mut dungeon_land, mut dungeon_nonzero) = (0, 0, 0);
    let cell_rev = dereth_dat::decompose::revision_over(&cell);
    let portal_rev = dereth_dat::decompose::revision_over(&portal);
    if args.out.exists() {
        std::fs::remove_dir_all(&args.out).map_err(|e| format!("{}: {e}", args.out.display()))?;
    }
    std::fs::create_dir_all(&args.out).map_err(|e| format!("{}: {e}", args.out.display()))?;
    // The cell records, with their versions: written once everything the overlay holds is known,
    // in an iteration numbered from it.
    let mut records = Vec::with_capacity(255 * 255);
    // AC1's content taken off the converted blocks, by block and in all; the rooms' numbers, by
    // block, deleted through the overlay where it writes no cell of its own in their place.
    let mut removed = cleared::Removed::default();
    let (mut removed_blocks, mut mixed_blocks, mut rooms_left_in_count) = (0usize, 0usize, 0usize);
    let mut removed_tsv = String::from(
        "block\tbuildings\trooms\tobjects\trestrictions\tcells_kept\trooms_left_in_count\n",
    );
    let mut removed_rooms: std::collections::BTreeMap<u32, Vec<u32>> =
        std::collections::BTreeMap::new();
    let mut versions = Vec::with_capacity(255 * 255);
    // Each block written and how many cells it has, for the placements' cells.
    let mut written_cells: std::collections::BTreeMap<(usize, usize), u32> =
        std::collections::BTreeMap::new();
    for bx in 0..255usize {
        for by in 0..255usize {
            #[allow(clippy::cast_possible_truncation)]
            let id = DataId(((bx as u32) << 24) | ((by as u32) << 16) | 0xFFFF);
            let ac1 = cell
                .read(id)
                .ok()
                .and_then(|b| CellLandblock::decode_payload(id, &b).ok());
            let placed = statics.by_block.get(&(bx, by));
            let with_rooms = ac2_buildings.get(&(bx, by));
            let vaults_here = vault_cells.get(&(bx, by));
            // AC1's landblock info, with its buildings, their rooms and its statics taken off: the
            // block's info is written whenever that takes something, so AC1's own is read through
            // only where it is what the converted block holds (an AC1 dungeon alone).
            let lid = DataId(id.raw() & 0xFFFF_0000 | 0xFFFE);
            let ac1_cleared = cell
                .read(lid)
                .ok()
                .and_then(|b| LandblockInfo::decode_payload(lid, &b).ok())
                .map(|i| {
                    cleared::clear(&i, &|k| {
                        cell.read(DataId(k)).ok().and_then(|b| {
                            dereth_assets::EnvCell::decode_payload(DataId(k), &b).ok()
                        })
                    })
                });
            if let Some(c) = ac1_cleared.as_ref().filter(|c| c.removed.any()) {
                removed.add(&c.removed);
                removed_blocks += 1;
                rooms_left_in_count += c.rooms_left_in_count;
                mixed_blocks += usize::from(c.removed.rooms > 0 && c.cells_kept > 0);
                let _ = writeln!(
                    removed_tsv,
                    "{bx:02X}{by:02X}\t{}\t{}\t{}\t{}\t{}\t{}",
                    c.removed.buildings,
                    c.removed.rooms,
                    c.removed.objects,
                    c.removed.restrictions,
                    c.cells_kept,
                    c.rooms_left_in_count
                );
                if !c.room_numbers.is_empty() {
                    removed_rooms.insert(lid.raw() & 0xFFFF_0000, c.room_numbers.clone());
                }
                // A room still inside the count, written hollow: whatever walks the count finds
                // every cell it names.
                let block = lid.raw() & 0xFFFF_0000;
                for &k in c
                    .room_numbers
                    .iter()
                    .filter(|&&k| k < 0x0100 + c.info.num_cells)
                {
                    let rid = DataId(block | k);
                    let room = cell
                        .read(rid)
                        .ok()
                        .and_then(|b| dereth_assets::EnvCell::decode_payload(rid, &b).ok())
                        .ok_or_else(|| format!("{rid:?}: a room in the count does not read"))?;
                    let version = cell
                        .base_entry(rid)
                        .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
                    versions.push(version);
                    records.push((rid, cleared::hollow(&room).encode()));
                }
            }
            let write_info = ac1_cleared.as_ref().is_some_and(|c| c.removed.any())
                || placed.is_some()
                || with_rooms.is_some()
                || vaults_here.is_some();
            let lbi_exists = ac1.as_ref().map_or(0, |l| l.lbi_exists) | u32::from(write_info);
            let (mut land, mut water, mut edge, mut basin) = (0, 0, false, false);
            let (mut nonzero, mut top) = (0, 0f32);
            for i in 0..=s {
                for j in 0..=s {
                    let k = (bx * s + i) * side + by * s + j;
                    let v = &w.vertex[k];
                    if v.water {
                        water += 1;
                    } else {
                        land += 1;
                    }
                    edge |= w.edge_rule[k];
                    basin |= v.floored;
                    if w.height[k] != 0 {
                        nonzero += 1;
                    }
                    top = top.max(chosen[usize::from(w.height[k])]);
                }
            }
            land_blocks += usize::from(land > 0);
            part_water += usize::from(land > 0 && water > 0);
            all_water += usize::from(land == 0);
            edge_blocks += usize::from(edge);
            basin_blocks += usize::from(basin);
            // AC1's dungeon test: all heights 0, cells, no buildings.
            if ac1
                .as_ref()
                .is_some_and(|l| l.lbi_exists != 0 && l.height.iter().all(|&h| h == 0))
            {
                let lid = DataId(id.raw() & 0xFFFF_0000 | 0xFFFE);
                let info = cell
                    .read(lid)
                    .ok()
                    .and_then(|b| LandblockInfo::decode_payload(lid, &b).ok());
                if let Some(info) = info.filter(|i| i.num_cells > 0 && i.buildings.is_empty()) {
                    dungeon_count += 1;
                    dungeon_land += usize::from(land > 0);
                    dungeon_nonzero += usize::from(nonzero > 0);
                    let _ = writeln!(
                        dungeons,
                        "{bx:02X}{by:02X}\t{}\t{land}\t{water}\t{nonzero}\t{top:.2}\t{}",
                        info.num_cells,
                        nonzero == 0
                    );
                }
            }
            let (words, hs, split) = w.block(bx, by);
            let rec = CellLandblock::from_dense(
                id,
                lbi_exists,
                DenseLand {
                    steps: STEPS,
                    terrain: words.into(),
                    height: hs.into(),
                    split: split.into(),
                },
            );
            let bytes = rec.encode();
            if bytes.len() != 1960 {
                return Err(format!("{id:?}: a dense record of {} bytes", bytes.len()));
            }
            let version = cell
                .base_entry(id)
                .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
            versions.push(version);
            records.push((id, bytes));
            // AC2's statics, AC2's buildings with rooms and the AC1 buildings copied in, added to
            // what is left of AC1's (its dungeon cells, where it has them).
            if write_info {
                let mut info = ac1_cleared.map_or_else(|| cleared::empty(lid), |c| c.info);
                info.objects.extend(placed.into_iter().flatten().cloned());
                for b in with_rooms.into_iter().flatten() {
                    for (rid, bytes) in rooms::add(&mut info, b) {
                        versions.push(1);
                        records.push((rid, bytes));
                    }
                }
                for m in vaults_here.into_iter().flatten() {
                    let first = 0x0100 + info.num_cells;
                    let at = |low: u16| m.numbers.get(&low).map(|&k| first + u32::from(k));
                    eprintln!(
                        "AC2 dungeon in {bx:02X}{by:02X}: cells {first:04X}..{:04X}; AC2's 0100 is {:04X?}, 017B is {:04X?}",
                        first + u32::try_from(m.cells.len()).unwrap_or(0) - 1,
                        at(0x0100),
                        at(0x017B)
                    );
                    for (rid, bytes) in rooms::add_cells(&mut info, &m.cells) {
                        versions.push(1);
                        records.push((rid, bytes));
                    }
                }
                written_cells.insert((bx, by), info.num_cells);
                let bytes = info.encode();
                let version = cell
                    .base_entry(lid)
                    .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
                versions.push(version);
                records.push((lid, bytes));
            }
        }
    }
    // AC1's rooms taken off, deleted from the cell file as read through the overlay, but for those
    // the overlay writes anew (AC2's rooms and the buildings copied in, numbered from the block's
    // kept cells).
    let deleted_rooms: Vec<DataId> = {
        let written: std::collections::HashSet<u32> =
            records.iter().map(|(id, _)| id.raw()).collect();
        removed_rooms
            .iter()
            .flat_map(|(&block, r)| r.iter().map(move |&k| block | k))
            .filter(|id| !written.contains(id))
            .map(DataId)
            .collect()
    };
    eprintln!(
        "AC1's content left off the converted blocks: {removed:?} in {removed_blocks} blocks; {} rooms deleted",
        deleted_rooms.len()
    );
    // AC2's ground pictures as the land slots' terrain textures, each slot by its own kind and
    // each other kind of the slot on the land by an entry of its own.
    eprintln!("taking AC2's ground pictures");
    let mut slot_counts = std::collections::BTreeMap::new();
    for v in w.vertex.iter().filter(|v| !v.water && !v.no_ground) {
        *slot_counts
            .entry(((v.word >> 2) & 0x1F, (v.word >> 7) & 0xF))
            .or_insert(0usize) += 1;
    }
    let ground = art::ground_textures(
        &ac2_art,
        &ac1_region,
        &slot_counts,
        &steep,
        grid.cell_len,
        &mut pictures,
        &mut ids,
        &mut art_records,
    )?;
    // AC2's road masks as AC1's road alpha maps, the road reaching as far into a converted square
    // as AC2's reaches into its own.
    let (road_maps, road_maps_tsv) = art::road_maps(
        &ac2_art,
        &ac1_region,
        g.square_len / grid.spacing(),
        &mut ids,
        &mut art_records,
    )?;
    // AC2's sky as AC1's, its models among the new records, and its tints beside the region.
    let sky = if args.sky {
        eprintln!("converting AC2's sky");
        let s = sky::convert(
            &args.ac2,
            &ac2_art,
            &ac1_region,
            args.sky_day,
            &mut ids,
            &mut art_records,
        )?;
        art_records.add(dereth_assets::sky_tints::SKY_TINTS, s.tints.encode());
        eprintln!("{}", s.line);
        Some(s)
    } else {
        None
    };
    // Every new record's id is free: the converted world names none of AC1's scenery records.
    check_free(&portal, art_records.0.keys().copied())?;
    let tm = ac1_region
        .land_surf
        .tex_merge
        .as_ref()
        .ok_or("AC1's region has no terrain textures")?;
    let patched = region::patch(
        &region_bytes,
        u32::try_from(STEPS).unwrap_or(3),
        ac2_road,
        &chosen,
    )?;
    let patched = crate::ac2_world::textures::append_terrain_textures(
        &patched,
        &tm.terrain_desc,
        &ground.extra,
    )?;
    let patched = crate::ac2_world::textures::patch_terrain_textures(
        &patched,
        &tm.terrain_desc,
        &ground.set,
    )?;
    let patched = crate::ac2_world::textures::patch_road_maps(&patched, &tm.road_maps, &road_maps)?;
    let patched = crate::ac2_world::scenes::splice(
        &patched,
        ac1_region.scene_info.as_deref().unwrap_or_default(),
        &scene_info,
        &ac1_region.terrain_types,
        &scene_lists,
    )?;
    let patched = match &sky {
        Some(s) => sky::splice(&patched, &s.groups)?,
        None => patched,
    };

    // The world's starting area: AC1's creation table with the one area, the dry land nearest the
    // start point on the converted land, named for the nearest of AC2's towns.
    eprintln!("placing the starting area");
    let labels = names::read_labels(&args.ac2)?;
    let block_records: std::collections::HashMap<u32, &[u8]> = records
        .iter()
        .filter(|(id, _)| id.raw() & 0xFFFF == 0xFFFF)
        .map(|(id, b)| (id.raw(), b.as_slice()))
        .collect();
    let mut blocks = std::collections::HashMap::new();
    let start_point = match &args.start {
        Some(p) => start::parse_start(
            &std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?,
        )?,
        None => start::start_point(),
    };
    let start_at = start::nearest_dry(start_point, |cell, local| {
        let id = cell.0 | 0xFFFF;
        let block = blocks.entry(id).or_insert_with(|| {
            let lb = CellLandblock::decode_payload(DataId(id), block_records.get(&id)?).ok()?;
            start::collision(&lb, STEPS, ac2_road, &chosen)
        });
        start::dry_ground(block.as_ref()?, cell, local)
    })
    .ok_or("no dry land near the start point")?;
    let (start_name, start_town_m) = start::nearest_town(&labels, start::world_point(&start_at))
        .ok_or("AC2's map names no town")?;
    let ac1_chargen = portal
        .read(DataId(CHARGEN_ID))
        .map_err(|e| format!("AC1 creation table: {e}"))?;
    let ac1_cg = dereth_assets::tables::CharGen::decode_payload(DataId(CHARGEN_ID), &ac1_chargen)
        .map_err(|e| format!("AC1 creation table: {e:?}"))?;
    if ac1_cg.encode() != ac1_chargen {
        return Err("AC1's creation table does not encode back to its own bytes".into());
    }
    let chargen = start::creation_table(&ac1_cg, &start_name, start_at).encode();
    let start_line = format!(
        "starting area: {start_name} (the nearest of AC2's towns, {start_town_m:.0} m away) at {:#010X} {:.3} {:.3} {:.3}, the dry land nearest world meters ({}, {}), {} m over the ground; AC1's creation table's only area, every heritage's start",
        start_at.cell.0,
        start_at.frame.origin.x,
        start_at.frame.origin.y,
        start_at.frame.origin.z,
        start_point.0,
        start_point.1,
        start::ABOVE_GROUND,
    );
    eprintln!("{start_line}");

    // The overlay's iteration, from everything it holds (the map picture is drawn from the land,
    // which is in it), so a changed overlay is a new iteration to a client holding an older one.
    let content = crate::ac2_world::iteration::digest(
        records
            .iter()
            .map(|(id, b)| (id.raw(), b.as_slice()))
            .chain(std::iter::once((REGION_ID, patched.as_slice())))
            .chain(std::iter::once((CHARGEN_ID, chargen.as_slice())))
            .chain(std::iter::once((
                TOMBSTONES_ID,
                dereth_dat::overlay::encode_tombstones(
                    &deleted_rooms
                        .iter()
                        .map(|id| dereth_dat::overlay::Tombstone {
                            id: id.raw(),
                            mask: 0,
                            iteration: 0,
                        })
                        .collect::<Vec<_>>(),
                )
                .as_slice(),
            )))
            .chain(art_records.0.iter().map(|(id, b)| (id.raw(), b.as_slice()))),
    );
    let ledger_path = &args.ledger;
    // A first build has no ledger yet: its content is the first generation.
    let ledger_text = match std::fs::read_to_string(ledger_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", ledger_path.display())),
    };
    let mut ledger = crate::ac2_world::iteration::Ledger::parse(&ledger_text)?;
    let (generation, new_content) = ledger.generation(&content);
    if new_content {
        std::fs::write(ledger_path, ledger.text())
            .map_err(|e| format!("{}: {e}", ledger_path.display()))?;
        eprintln!(
            "new content: generation {generation} added to {} (keep it for the next build)",
            ledger_path.display()
        );
    }
    let cell_iterations = crate::ac2_world::iteration::iterations(cell_rev, generation);
    let portal_iterations = crate::ac2_world::iteration::iterations(portal_rev, generation);
    let cell_iteration = *cell_iterations.last().unwrap_or(&cell_rev);
    let portal_iteration = *portal_iterations.last().unwrap_or(&portal_rev);
    eprintln!(
        "generation {generation}: iterations to cell {cell_iteration}, portal {portal_iteration}"
    );
    let mut cw = OverlayWriter::open_or_create(
        &args.out.join(container_name(ModernDat::Cell)),
        &cell,
        "client_cell_1.dat",
        WORLD_KEY,
        0,
    )
    .map_err(|e| format!("cell overlay: {e:?}"))?;
    for id in &deleted_rooms {
        cw.tombstone(*id, 0, cell_iteration)
            .map_err(|e| format!("{id:?}: {e:?}"))?;
    }
    for ((id, bytes), version) in records.iter().zip(&versions) {
        cw.put(*id, bytes, *version, cell_iteration, 0)
            .map_err(|e| format!("{id:?}: {e:?}"))?;
    }
    for i in &cell_iterations {
        cw.add_iteration(*i, 0).map_err(|e| format!("{e:?}"))?;
    }
    cw.flush(0).map_err(|e| format!("{e:?}"))?;
    drop(cw);
    let mut pw = OverlayWriter::open_or_create(
        &args.out.join(container_name(ModernDat::Portal)),
        &portal,
        "client_portal.dat",
        WORLD_KEY,
        0,
    )
    .map_err(|e| format!("portal overlay: {e:?}"))?;
    let rversion = portal
        .base_entry(DataId(REGION_ID))
        .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
    pw.put(DataId(REGION_ID), &patched, rversion, portal_iteration, 0)
        .map_err(|e| format!("region: {e:?}"))?;
    let cversion = portal
        .base_entry(DataId(CHARGEN_ID))
        .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
    pw.put(DataId(CHARGEN_ID), &chargen, cversion, portal_iteration, 0)
        .map_err(|e| format!("creation table: {e:?}"))?;
    for (id, bytes) in &art_records.0 {
        let version = portal
            .base_entry(*id)
            .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
        pw.put(*id, bytes, version, portal_iteration, 0)
            .map_err(|e| format!("{id:?}: {e:?}"))?;
    }
    for i in &portal_iterations {
        pw.add_iteration(*i, 0).map_err(|e| format!("{e:?}"))?;
    }
    pw.flush(0).map_err(|e| format!("{e:?}"))?;
    drop(pw);

    // The map window's picture, drawn from the land as the overlay now holds it.
    eprintln!("drawing the map");
    let (map_record, map_rgb, map_size) = map_picture(&args.ac1, &args.out, &portal)?;
    let mut pw = OverlayWriter::open_or_create(
        &args.out.join(container_name(ModernDat::Portal)),
        &portal,
        "client_portal.dat",
        WORLD_KEY,
        0,
    )
    .map_err(|e| format!("portal overlay: {e:?}"))?;
    let mversion = portal
        .base_entry(DataId(MAP_ID))
        .map_or(1, |e| u16::try_from(e.bits >> 16).unwrap_or(1).max(1));
    pw.put(DataId(MAP_ID), &map_record, mversion, portal_iteration, 0)
        .map_err(|e| format!("map picture: {e:?}"))?;
    pw.flush(0).map_err(|e| format!("{e:?}"))?;
    drop(pw);
    write_png(
        &beside_of(&args.out).join("map-picture.png"),
        &map_rgb,
        map_size,
    )?;
    // The world's own rules, which Empyrean reads from the overlay folder: AC2's land lies over
    // AC1's dungeons, so a position is in a dungeon by its cell (Empyrean's V563).
    let rules = args.out.join(WORLD_RULES);
    std::fs::write(&rules, WORLD_RULES_TEXT).map_err(|e| format!("{}: {e}", rules.display()))?;
    // The things the world places itself, which Empyrean reads from the overlay folder (V565).
    let placed = args.out.join(PLACEMENTS);
    if let Some(p) = &args.placements {
        let text = std::fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()))?;
        let n = check_placements(&text, &written_cells)?;
        std::fs::write(&placed, &text).map_err(|e| format!("{}: {e}", placed.display()))?;
        eprintln!("{PLACEMENTS}: {n} placements");
    } else if placed.exists() {
        std::fs::remove_file(&placed).map_err(|e| format!("{}: {e}", placed.display()))?;
    }
    // AC2's towns and landmarks, which Dereth's map names in place of AC1's notes.
    let places = args.out.join(PLACES);
    let named = if args.names {
        eprintln!("naming AC2's places");
        let (doc, towns, landmarks) =
            names::document(&labels, WORLD_KEY, |x, y| w.height_at(x, y, true));
        std::fs::write(&places, doc).map_err(|e| format!("{}: {e}", places.display()))?;
        format!(
            "{PLACES}: {towns} towns and {landmarks} landmarks of AC2's {} map labels",
            labels.len()
        )
    } else {
        if places.exists() {
            std::fs::remove_file(&places).map_err(|e| format!("{}: {e}", places.display()))?;
        }
        format!("{PLACES}: none (--names off)")
    };

    // Read it all back through the overlay over AC1's own files.
    eprintln!("verifying");
    let od = OverlayDir::new(&args.out).map_err(|e| format!("{e:?}"))?;
    let cl = od
        .layer_over(ModernDat::Cell, &cell, Some(WORLD_KEY))
        .map_err(|e| format!("{e:?}"))?
        .ok_or("no cell overlay")?;
    for (id, bytes) in &records {
        let got = cl.read(*id).map_err(|e| format!("{id:?}: {e:?}"))?;
        if &got != bytes {
            return Err(format!("{id:?} reads back otherwise"));
        }
        if id.raw() & 0xFFFF == 0xFFFE {
            LandblockInfo::decode_payload(*id, &got).map_err(|e| format!("{id:?}: {e:?}"))?;
            continue;
        }
        // A room of a building copied in.
        if id.raw() & 0xFFFF < 0xFFFE {
            let c = dereth_assets::EnvCell::decode_payload(*id, &got)
                .map_err(|e| format!("{id:?}: {e:?}"))?;
            if c.encode() != *bytes {
                return Err(format!("{id:?} does not decode to itself"));
            }
            continue;
        }
        let d = CellLandblock::decode_payload(*id, &got).map_err(|e| format!("{id:?}: {e:?}"))?;
        if d.vertex_steps() != STEPS || d.encode() != *bytes {
            return Err(format!("{id:?} does not decode to itself"));
        }
    }
    // The rooms taken off are not there, and no doorway, way between cells or cell seen from
    // another on the converted land names one.
    for id in &deleted_rooms {
        if cl.read(*id).is_ok() {
            return Err(format!("{id:?}: a room taken off still reads"));
        }
    }
    let gone = |id: u32| deleted_rooms.binary_search(&DataId(id)).is_ok();
    let mut points_at_gone = Vec::new();
    for (id, bytes) in records.iter().filter(|(id, _)| id.raw() & 0xFFFF == 0xFFFE) {
        let block = id.raw() & 0xFFFF_0000;
        let info =
            LandblockInfo::decode_payload(*id, bytes).map_err(|e| format!("{id:?}: {e:?}"))?;
        for p in info.buildings.iter().flat_map(|b| &b.portals) {
            for k in std::iter::once(p.other_cell_id).chain(p.stab_list.iter().copied()) {
                if gone(block | u32::from(k)) {
                    points_at_gone.push(format!("{:08X} by a doorway", block | u32::from(k)));
                }
            }
        }
        for k in 0x0100..0x0100 + info.num_cells {
            let Ok(b) = cl.read(DataId(block | k)) else {
                continue;
            };
            let c = dereth_assets::EnvCell::decode_payload(DataId(block | k), &b)
                .map_err(|e| format!("{:08X}: {e:?}", block | k))?;
            let ways = c
                .portals
                .iter()
                .filter(|p| p.flags & 4 == 0)
                .map(|p| p.other_cell_id & 0xFFFF);
            for to in ways.chain(c.visible_cells.iter().map(|&v| u32::from(v))) {
                if gone(block | to) {
                    points_at_gone.push(format!("{:08X} from {:08X}", block | to, block | k));
                }
            }
        }
    }
    if !points_at_gone.is_empty() {
        return Err(format!(
            "{} references to rooms taken off: {:?}",
            points_at_gone.len(),
            &points_at_gone[..points_at_gone.len().min(20)]
        ));
    }
    if gone(start_at.cell.0) {
        return Err(format!(
            "the starting area {:#010X} is in a room taken off",
            start_at.cell.0
        ));
    }
    let pl = od
        .layer_over(ModernDat::Portal, &portal, Some(WORLD_KEY))
        .map_err(|e| format!("{e:?}"))?
        .ok_or("no portal overlay")?;
    let r = Region::decode_payload(
        DataId(REGION_ID),
        &pl.read(DataId(REGION_ID)).map_err(|e| format!("{e:?}"))?,
    )
    .map_err(|e| format!("converted region: {e:?}"))?;
    let rl = &r.land_defs;
    if rl.vertex_per_cell != 3
        || rl.road_width != ac2_road
        || rl.land_height_table != chosen.to_vec()
    {
        return Err(format!(
            "the converted region's land definitions read back otherwise: {rl:?}"
        ));
    }
    // The terrain textures: each type set reads back as set, every other entry as AC1's, and the
    // variants' entries after them.
    let rtm = r
        .land_surf
        .tex_merge
        .as_ref()
        .ok_or("the converted region lost its terrain textures")?;
    if rtm.terrain_desc.len() != tm.terrain_desc.len() + ground.extra.len()
        || rtm.terrain_desc[tm.terrain_desc.len()..] != ground.extra[..]
    {
        return Err("the variants' terrain textures read back otherwise".into());
    }
    for (d, a) in rtm.terrain_desc.iter().zip(&tm.terrain_desc) {
        let mut want = *a;
        if let Some(&(tex, tile)) = ground.set.get(&a.terrain_type) {
            want.tex_gid = DataId(tex);
            want.tex_tiling = tile;
        }
        if *d != want {
            return Err(format!(
                "terrain texture {} reads back otherwise",
                a.terrain_type
            ));
        }
    }
    // The road maps: each code's texture as made, in AC1's order.
    let want_maps: Vec<(u32, u32)> = tm
        .road_maps
        .iter()
        .map(|m| {
            (
                m.code,
                road_maps.get(&m.code).copied().unwrap_or(m.tex_gid.raw()),
            )
        })
        .collect();
    if rtm
        .road_maps
        .iter()
        .map(|m| (m.code, m.tex_gid.raw()))
        .collect::<Vec<_>>()
        != want_maps
    {
        return Err("the road maps read back otherwise".into());
    }
    let mut same = r.clone();
    same.land_defs = ac1_region.land_defs.clone();
    same.land_surf = ac1_region.land_surf.clone();
    // The scene types read back as made, and every terrain type's list as set; names and colors
    // as AC1's.
    if r.scene_info.as_ref() != Some(&scene_info) {
        return Err("the region's scene types read back otherwise".into());
    }
    for ((t, a), list) in r
        .terrain_types
        .iter()
        .zip(&ac1_region.terrain_types)
        .zip(&scene_lists)
    {
        if t.scene_types != *list
            || t.terrain_name != a.terrain_name
            || t.terrain_color != a.terrain_color
        {
            return Err(format!(
                "terrain type {} reads back with other scene types",
                t.terrain_name
            ));
        }
    }
    same.scene_info = ac1_region.scene_info.clone();
    same.terrain_types = ac1_region.terrain_types.clone();
    // The sky reads back as made, its tick sizes AC1's.
    if let Some(s) = &sky {
        let rs = r
            .sky_info
            .as_ref()
            .ok_or("the converted region lost its sky")?;
        let a1 = ac1_region
            .sky_info
            .as_ref()
            .ok_or("AC1's region has no sky")?;
        if rs.day_groups != s.groups
            || rs.tick_size != a1.tick_size
            || rs.light_tick_size != a1.light_tick_size
        {
            return Err("the converted region's sky reads back otherwise".into());
        }
        same.sky_info = ac1_region.sky_info.clone();
    }
    if same != ac1_region {
        return Err("the converted region differs from AC1's outside its land definitions and terrain textures".into());
    }
    for (id, bytes) in &art_records.0 {
        if pl.read(*id).map_err(|e| format!("{id:?}: {e:?}"))? != *bytes {
            return Err(format!("{id:?} reads back otherwise"));
        }
        // Every new record decodes as what it is.
        let decoded = match id.raw() >> 24 {
            0x01 => dereth_assets::GfxObj::decode_payload(*id, bytes).map(|_| ()),
            0x02 => dereth_assets::Setup::decode_payload(*id, bytes).map(|_| ()),
            0x05 => dereth_assets::SurfaceTexture::decode_payload(*id, bytes).map(|_| ()),
            0x06 => RenderSurface::decode_payload(*id, bytes).map(|_| ()),
            0x08 => dereth_assets::Surface::decode_payload(*id, bytes).map(|_| ()),
            0x11 => {
                dereth_assets::motion::GfxObjDegradeInfo::decode_payload(*id, bytes).map(|_| ())
            }
            _ if *id == dereth_assets::sky_tints::SKY_TINTS => {
                dereth_assets::sky_tints::SkyTints::decode(bytes).map(|_| ())
            }
            _ => Ok(()),
        };
        decoded.map_err(|e| format!("{id:?} does not decode: {e:?}"))?;
    }
    for tex in ground
        .set
        .values()
        .map(|&(t, _)| t)
        .chain(ground.extra.iter().map(|d| d.tex_gid.raw()))
    {
        let st = dereth_assets::SurfaceTexture::decode_payload(
            DataId(tex),
            &pl.read(DataId(tex)).map_err(|e| format!("{e:?}"))?,
        )
        .map_err(|e| format!("{tex:08X}: {e:?}"))?;
        let rid = st.source_levels[0];
        let rb = pl.read(rid).map_err(|e| format!("{e:?}"))?;
        let rs = RenderSurface::decode_payload(rid, &rb).map_err(|e| format!("{rid:?}: {e:?}"))?;
        if rs.payload(&rb).is_none_or(<[u8]>::is_empty) {
            return Err(format!("{rid:?} holds no pixels"));
        }
    }
    // The creation table: AC1's, with the one area.
    let cg = pl
        .read(DataId(CHARGEN_ID))
        .map_err(|e| format!("creation table: {e:?}"))?;
    let read_cg = dereth_assets::tables::CharGen::decode_payload(DataId(CHARGEN_ID), &cg)
        .map_err(|e| format!("converted creation table: {e:?}"))?;
    if cg != chargen
        || read_cg != start::creation_table(&ac1_cg, &start_name, start_at)
        || read_cg.starter_areas.len() != 1
    {
        return Err("the creation table reads back otherwise".into());
    }
    let m = pl.read(DataId(MAP_ID)).map_err(|e| format!("{e:?}"))?;
    let ms = RenderSurface::decode_payload(DataId(MAP_ID), &m)
        .map_err(|e| format!("map picture: {e:?}"))?;
    if m != map_record || ms.payload(&m).map(<[u8]>::len) != Some(map_size.0 * map_size.1 * 3) {
        return Err("the map picture reads back otherwise".into());
    }

    // Container digests, for the README and the determinism check.
    let digests: Vec<String> = od
        .containers()
        .iter()
        .map(|p| {
            sha256(p).map(|d| {
                format!(
                    "{d}  {}",
                    p.file_name().unwrap_or_default().to_string_lossy()
                )
            })
        })
        .collect::<Result<_, _>>()?;

    let beside = beside_of(&args.out);
    let mut stats = String::new();
    let _ = writeln!(
        stats,
        "AC2's landmass in AC1 (true size, {STEPS} vertex steps a 24 m cell)"
    );
    let _ = writeln!(
        stats,
        "AC1 blocks written: 65025 (all dense, 1960 bytes each)"
    );
    let _ = writeln!(
        stats,
        "AC2 placeholder land left out as sea (the AC2 Horizon map's rule):"
    );
    for p in &dropped {
        let _ = writeln!(
            stats,
            "  {:?}: AC2 blocks {:02X}{:02X}-{:02X}{:02X} (x {:02X}-{:02X}, y {:02X}-{:02X}), {} blocks",
            p.why,
            p.south_west.0,
            p.south_west.1,
            p.north_east.0,
            p.north_east.1,
            p.south_west.0,
            p.north_east.0,
            p.south_west.1,
            p.north_east.1,
            p.blocks
        );
    }
    let _ = writeln!(
        stats,
        "  AC1 land blocks: {} with it, {} without; all {} lost lie over the stretches left out, none gained",
        land_before.iter().filter(|&&b| b).count(),
        land_after.iter().filter(|&&b| b).count(),
        lost.len()
    );
    let _ = writeln!(stats, "land blocks (any land vertex): {land_blocks}");
    let _ = writeln!(stats, "  partly water: {part_water}");
    let _ = writeln!(stats, "entirely water blocks: {all_water}");
    let _ = writeln!(stats, "vertices (world, unique): {} land, {water_vertices} water ({water_vertices_in_ac2} inside AC2's world)", land_vertices);
    let _ = writeln!(stats, "basin: land vertices AC2 has below its sea, floored at 0 m: {floored} in {basin_blocks} AC1 blocks");
    let _ = writeln!(stats, "edge rule (lower water plane wins): {edge_vertices} vertices changed, in {edge_blocks} AC1 blocks");
    let _ = writeln!(
        stats,
        "height table: {} ({} m to {} m)",
        args.table, chosen[1], chosen[255]
    );
    let _ = writeln!(stats, "  fits by greatest error allowed:{ladder}");
    let _ = writeln!(stats, "  fitted with the greatest rounding error at most {MAX_ROUNDING} m: mean {:.3} m, max {:.3} m over the converted land heights", fit.mean, fit.max);
    let _ = writeln!(
        stats,
        "rounding error on land vertices, fitted table: {rounding_fitted}"
    );
    let _ = writeln!(
        stats,
        "rounding error on land vertices, AC2's own table lowered: {rounding_lowered}"
    );
    let _ = writeln!(
        stats,
        "loss at AC2's land vertices, resampling only: {loss_exact}"
    );
    let _ = writeln!(
        stats,
        "loss at AC2's land vertices, resampled and rounded: {loss_rounded}"
    );
    let _ = writeln!(
        stats,
        "  of the {} over 5 m, {big_steep} rise more than 10 m to a neighbor 10 m away (cliffs, spikes)",
        big.len()
    );
    let _ = writeln!(
        stats,
        "AC1 dungeon blocks (all heights 0, cells, no buildings): {dungeon_count}"
    );
    let _ = writeln!(
        stats,
        "AC1's content left off the converted blocks, in {removed_blocks} blocks (ac1-removed.tsv): {} buildings, {} rooms, {} statics, {} house restriction entries",
        removed.buildings, removed.rooms, removed.objects, removed.restrictions
    );
    let _ = writeln!(
        stats,
        "  blocks with rooms taken and AC1 cells kept (a dungeon no building reaches): {mixed_blocks}; rooms numbered below a kept cell (inside its block's cell count, written hollow): {rooms_left_in_count}"
    );
    let _ = writeln!(
        stats,
        "  rooms deleted from the cell file through the overlay: {}; written anew as AC2's or copied rooms, or hollow: {}",
        deleted_rooms.len(),
        removed.rooms - deleted_rooms.len()
    );
    let _ = writeln!(
        stats,
        "  with converted land: {dungeon_land}; no longer all heights 0: {dungeon_nonzero}"
    );
    let _ = writeln!(
        stats,
        "map window picture {MAP_ID:#010X}: AC1's frame, the converted land drawn into its marker box (map-picture.png)"
    );
    let _ = writeln!(stats, "{named}");
    let _ = writeln!(stats, "{start_line}");
    let _ = writeln!(
        stats,
        "terrain textures from AC2's ground: {} (AC1's land slots and the road), {} more for the slots' other kinds, their steep ground and AC2's roads 2 and 3 (ground.tsv)",
        ground.set.len(),
        ground.extra.len()
    );
    let _ = writeln!(
        stats,
        "road alpha maps from AC2's road masks: {} (road-maps.tsv)",
        road_maps.len()
    );
    let _ = writeln!(stats, "{cover_line} (cover-kinds.tsv, cover-setups.tsv)");
    let _ = writeln!(
        stats,
        "overlay records added or written over for them: {}",
        art_records.0.len()
    );
    let mut by_kind: std::collections::BTreeMap<u32, (usize, usize)> =
        std::collections::BTreeMap::new();
    for (id, b) in &art_records.0 {
        let e = by_kind.entry(id.raw() >> 24).or_default();
        e.0 += 1;
        e.1 += b.len();
    }
    for (k, (n, bytes)) in &by_kind {
        let _ = writeln!(
            stats,
            "  {k:02X}: {n} records, {:.1} MB",
            *bytes as f64 / 1e6
        );
    }
    let _ = writeln!(
        stats,
        "AC2's statics: {} placed, {} models; left out: {:?} (statics.tsv)",
        statics.placed, statics.models, statics.left_out
    );
    let _ = writeln!(stats, "  their models' materials, by how they are drawn (models, placements; statics-surfaces.tsv):");
    for (way, (models, placements)) in &statics.surfaces {
        let _ = writeln!(stats, "    {way}: {models} models, {placements} placements");
    }
    let _ = writeln!(
        stats,
        "  their models, by how they were made (models, placements; statics-made.tsv):"
    );
    for (way, (models, placements)) in &statics.made {
        let _ = writeln!(stats, "    {way}: {models} models, {placements} placements");
    }
    let _ = writeln!(
        stats,
        "  of them, {} draw nothing but an effect, made AC1 objects that play its particles, and {} draw and play effects too (statics-effects.tsv)",
        statics.effects, statics.mesh_effects
    );
    let _ = writeln!(stats, "{buried_line}");
    let _ = writeln!(stats, "{}", climb.line);
    let _ = writeln!(
        stats,
        "{}",
        sky.as_ref()
            .map_or("sky: AC1's (--sky ac1)", |s| s.line.as_str())
    );
    let _ = writeln!(
        stats,
        "overlay content {} is generation {generation} (generations.tsv): iterations {cell_rev}..={cell_iteration} over the cell file, {portal_rev}..={portal_iteration} over the portal file",
        crate::ac2_world::iteration::hex(&content)
    );
    let _ = writeln!(stats, "overlay containers:");
    for d in &digests {
        let _ = writeln!(stats, "  {d}");
    }
    // Inside AC2's world: the water's kinds and the land's slots.
    let mut by_slot = [0usize; 32];
    let mut roads = 0usize;
    for v in w.vertex.iter().filter(|v| !v.no_ground) {
        by_slot[usize::from((v.word >> 2) & 0x1F)] += 1;
        roads += usize::from(!v.water && v.word & 3 != 0);
    }
    let _ = writeln!(
        stats,
        "water vertices inside AC2's world by slot: running {}, fresh {}, shallow sea {}, shallow still sea {}, deep sea {}",
        by_slot[16], by_slot[17], by_slot[18], by_slot[19], by_slot[20]
    );
    // Each kind of water, wading and deep; and the ground under it kept land at the water's height.
    let mut by_class = std::collections::BTreeMap::<(kinds::WaterKind, bool), usize>::new();
    let mut shore = std::collections::BTreeMap::<kinds::WaterKind, usize>::new();
    for v in w.vertex.iter().filter(|v| !v.no_ground) {
        if let Some(k) = v.water_kind {
            *by_class
                .entry((k, kinds::is_wading((v.word >> 2) & 0x1F)))
                .or_default() += 1;
        }
    }
    let step = w.grid.spacing();
    for (k, _) in w.vertex.iter().enumerate().filter(|(_, v)| v.shore) {
        // The kind of the water it was under.
        let (x, y) = (
            (k / w.grid.side()) as f64 * step,
            (k % w.grid.side()) as f64 * step,
        );
        let near = g.nearest_vertex(x, y).map_or(0, |n| g.kind[n]);
        let plane = g.plane(x, y).0.unwrap_or(f64::from(table::SEA));
        let sea = (plane - f64::from(table::SEA)).abs() < 0.005;
        *shore
            .entry(kinds::water_kind(u32::from(near), sea))
            .or_default() += 1;
    }
    let class = |k, wading| by_class.get(&(k, wading)).copied().unwrap_or(0);
    let _ = writeln!(
        stats,
        "water vertices inside AC2's world by kind and depth (wading under {} m, deep from it): river {} wading, {} deep; lake {} wading, {} deep; sea {} wading, {} deep",
        kinds::WADING_DEPTH,
        class(kinds::WaterKind::River, true),
        class(kinds::WaterKind::River, false),
        class(kinds::WaterKind::Lake, true),
        class(kinds::WaterKind::Lake, false),
        class(kinds::WaterKind::Sea, true),
        class(kinds::WaterKind::Sea, false),
    );
    let at = |k| shore.get(&k).copied().unwrap_or(0);
    let _ = writeln!(
        stats,
        "shore: vertices under AC2's water kept land at its height, AC2's bank standing over {} m above it halfway to land: {} (river {}, lake {}, sea {})",
        crate::ac2_world::dense::SHORE_RISE,
        shore.values().sum::<usize>(),
        at(kinds::WaterKind::River),
        at(kinds::WaterKind::Lake),
        at(kinds::WaterKind::Sea),
    );
    let _ = writeln!(stats, "road vertices: {roads}");
    let slots: Vec<String> = (0..32)
        .filter(|&k| !(16..=20).contains(&k) && by_slot[k] > 0)
        .map(|k| format!("{k}:{}", by_slot[k]))
        .collect();
    let _ = writeln!(stats, "land vertices by AC1 slot: {}", slots.join(" "));
    let mut kinds_tsv = String::from("ac2_kind\tac1_slot\tvariant\n");
    for (k, (slot, v)) in kinds::kinds().slots.iter().enumerate() {
        let _ = writeln!(kinds_tsv, "{k}\t{slot}\t{v}");
    }
    let mut readme = String::new();
    let _ = writeln!(
        readme,
        "AC2's landmass in AC1's formats: the world overlay `{WORLD_KEY}` in overlay/."
    );
    let _ = writeln!(
        readme,
        "Made from your own installs' files: it holds their content, so it is for your own server."
    );
    let _ = writeln!(readme);
    let _ = writeln!(readme, "Made by:");
    let _ = writeln!(readme, "  {}", args.command);
    let _ = writeln!(readme, "The same inputs give the same output bytes.");
    if args.any_install {
        let _ = writeln!(
            readme,
            "Built with --any-install: an input that is not the expected file may give a world that matches no one else's."
        );
    }
    let _ = writeln!(readme);
    let _ = writeln!(readme, "Inputs (SHA-256):");
    for f in &fingerprints {
        let _ = writeln!(readme, "  {f}");
    }
    let _ = writeln!(readme);
    let _ = writeln!(readme, "Outputs (SHA-256):");
    for d in &digests {
        let _ = writeln!(readme, "  {d}");
    }
    let _ = writeln!(readme);
    let _ = writeln!(
        readme,
        "AC2's placeholder land (the copied islands in the far north, the flat specks) is left out as sea: see stats.txt."
    );
    let _ = writeln!(readme, "overlay/world.toml: the world's own rules for Empyrean (dungeons_by_cells: AC2's land lies over AC1's dungeons, so a position is in a dungeon by its cell; own_placements: none of ACE's world database is placed in it).");
    if let Some(p) = &args.placements {
        let _ = writeln!(readme, "overlay/placements.tsv: the things the world places itself, from {} (Empyrean's own placements; a portal leads where the portaldestination it links to stands).", p.display());
    }
    let _ = writeln!(readme, "The portal overlay's character-creation table (0x0E000002) offers one starting area: see stats.txt.");
    let _ = writeln!(readme, "overlay/places.tsv: AC2's towns and landmarks where AC2's map names them, which Dereth's map shows in place of AC1's notes (on by default; --names off leaves it out).");
    let _ = writeln!(readme, "The converted blocks hold none of AC1's buildings, their rooms, its statics or its house restrictions (ac1-removed.tsv; AC1's dungeons under the land keep their cells).");
    let _ = writeln!(readme, "Beside it: stats.txt, dungeons.tsv, ac1-removed.tsv, worst-loss.tsv, kinds.tsv, map-picture.png (the map window's picture 0x0600127D, also in the portal overlay), ground.tsv (each terrain slot's AC2 ground picture), road-maps.tsv (each road alpha map's AC2 mask), cover-kinds.tsv (each of AC2's cover kinds: what AC2 grows on it, and its samples), cover-setups.tsv (each AC1 setup the cover's scenes place: its AC2 object, lift and least slope).");
    for (name, body) in [
        ("README.txt", &readme),
        ("stats.txt", &stats),
        ("dungeons.tsv", &dungeons),
        ("ac1-removed.tsv", &removed_tsv),
        ("worst-loss.tsv", &worst_tsv),
        ("kinds.tsv", &kinds_tsv),
        ("ground.tsv", &ground.tsv),
        ("road-maps.tsv", &road_maps_tsv),
        ("cover-kinds.tsv", &cover_kinds_tsv),
        ("cover-setups.tsv", &made.tsv),
        ("cover-samples.tsv", &cover_samples_tsv),
        ("statics.tsv", &statics.tsv),
        ("statics-blocks.tsv", &statics_blocks),
        ("statics-showing.tsv", &statics.showing_tsv),
        ("statics-surfaces.tsv", &statics.surfaces_tsv),
        ("statics-made.tsv", &statics.made_tsv),
        ("statics-left.tsv", &statics.left_tsv),
        ("statics-effects.tsv", &statics.effects_tsv),
        ("ledges.tsv", &climb.tsv),
        ("ramps.tsv", &climb.ramps_tsv),
        (
            "sky.tsv",
            &sky.as_ref().map(|s| s.tsv.clone()).unwrap_or_default(),
        ),
    ] {
        let p = beside.join(name);
        std::fs::write(&p, body).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    print!("{stats}");
    eprintln!("done in {:.0} s", t0.elapsed().as_secs_f64());
    Ok(())
}

/// Refuses a new record's id that AC1's portal file or the client's own records already hold.
fn check_free(portal: &DatFile, ids: impl Iterator<Item = DataId>) -> Result<(), String> {
    let layers = dereth_client_runtime::assets::client_layers();
    let taken: std::collections::BTreeSet<DataId> = layers.iter().flat_map(|l| l.ids()).collect();
    for id in ids {
        if portal.read(id).is_ok() || taken.contains(&id) {
            return Err(format!("{id:?} is already taken"));
        }
    }
    Ok(())
}

/// Per AC1 block (x major), whether it holds a land vertex.
fn land_blocks(w: &DenseWorld) -> Vec<bool> {
    let s = w.grid.block_squares();
    let side = w.grid.side();
    let b = w.grid.blocks;
    (0..b * b)
        .map(|k| {
            let (bx, by) = (k / b, k % b);
            (0..=s).any(|i| (0..=s).any(|j| !w.vertex[(bx * s + i) * side + by * s + j].water))
        })
        .collect()
}

/// A map picture: its record, its pixels as RGB, and its width and height.
type MapPicture = (Vec<u8>, Vec<u8>, (usize, usize));

/// The folder the overlay folder sits in, where the notes beside it go.
/// A landblock's info with nothing in it ([`cleared::empty`]).
fn cleared_empty(id: DataId) -> LandblockInfo {
    cleared::empty(id)
}

fn beside_of(out: &Path) -> PathBuf {
    out.parent().unwrap_or(Path::new(".")).to_path_buf()
}

/// The map window's picture over AC1's files with the overlay in `out` laid on: the retail
/// picture's frame with the converted land in its marker box, twice as many times as large as the
/// map pages enlarge the land ([`map::enlargement`]). The record, its pixels as RGB, and its size.
fn map_picture(ac1: &Path, out: &Path, portal: &DatFile) -> Result<MapPicture, String> {
    let id = DataId(MAP_ID);
    let bytes = portal
        .read(id)
        .map_err(|e| format!("AC1 map picture: {e}"))?;
    let rs =
        RenderSurface::decode_payload(id, &bytes).map_err(|e| format!("AC1 map picture: {e:?}"))?;
    if rs.format != PFID_R8G8B8 {
        return Err(format!(
            "AC1's map picture is in pixel format {}, not 20",
            rs.format
        ));
    }
    let (w, h) = (rs.width as usize, rs.height as usize);
    let px = rs.payload(&bytes).ok_or("AC1's map picture is short")?;
    if px.len() != w * h * 3 {
        return Err("AC1's map picture is not its size".into());
    }
    let rgb: Vec<u8> = px.chunks(3).flat_map(|p| [p[2], p[1], p[0]]).collect();
    let store = dereth_dat::RetailDatStore::open_dir(ac1)
        .map_err(|e| format!("{e:?}"))?
        .with_overlay(
            &OverlayDir::new(out).map_err(|e| format!("{e:?}"))?,
            Some(WORLD_KEY),
        )
        .map_err(|e| format!("{e:?}"))?;
    let built =
        dereth_scene::world_map::MapBuild::start(std::sync::Arc::new(store), true).wait()?;
    let ov = built.overview.ok_or("the map drew no overview")?;
    let side = ov.width as usize;
    // The map pages stretch the part of the world the land takes over the whole box: the picture
    // is drawn twice as many times as large, so they show it at its own size on an interface
    // drawn up to twice as large.
    #[allow(clippy::cast_precision_loss)] // LINT-OK: 6123.
    let k = map::enlargement(built.map.frame().side, dereth_scene::world_map::SIDE as f64);
    let frame = map::enlarge(&rgb, (w, h), k);
    let (w, h) = (w * k, h * k);
    let pic = map::compose(
        &frame,
        (w, h),
        map::SHIPPED.scaled(k),
        &ov.bgra,
        side,
        dereth_scene::world_map::VERTS,
    );
    let bgr: Vec<u8> = pic.chunks(3).flat_map(|p| [p[2], p[1], p[0]]).collect();
    let rs = RenderSurface {
        width: u32::try_from(w).map_err(|e| e.to_string())?,
        height: u32::try_from(h).map_err(|e| e.to_string())?,
        ..rs
    };
    let record = rs.encode(&bgr);
    Ok((record, pic, (w, h)))
}

/// An RGB picture as a PNG.
fn write_png(path: &Path, rgb: &[u8], (w, h): (usize, usize)) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut e = png::Encoder::new(
        std::io::BufWriter::new(f),
        u32::try_from(w).unwrap_or(0),
        u32::try_from(h).unwrap_or(0),
    );
    e.set_color(png::ColorType::Rgb);
    e.set_depth(png::BitDepth::Eight);
    e.write_header()
        .and_then(|mut wr| wr.write_image_data(rgb))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn par_rows<T: Send, F: Fn(usize) -> T + Sync>(n: usize, f: F) -> Vec<T> {
    crate::ac2_world::dense::par_map(n, f)
}

/// The file of the world's own rules in its overlay folder, as Empyrean reads it.
const WORLD_RULES: &str = "world.toml";
/// The file in the overlay folder that names the world's towns and landmarks, as Dereth reads it.
const PLACES: &str = "places.tsv";
/// The file in the overlay folder holding the things the world places itself, as Empyrean reads
/// it: one landblock instance a line, `guid wcid cell x y z qw qx qy qz [links]`, tab-separated.
const PLACEMENTS: &str = "placements.tsv";

/// How many placements `text` holds, each in a cell the world has: an outdoor cell of any block,
/// or one of the cells of a block this world writes (`written`, its cell count).
///
/// # Errors
/// A line that does not read, or a cell the world does not have.
fn check_placements(
    text: &str,
    written: &std::collections::BTreeMap<(usize, usize), u32>,
) -> Result<usize, String> {
    let mut n = 0;
    for (k, line) in text.lines().enumerate() {
        if line.trim().is_empty() || line.starts_with('#') || line.starts_with("guid\t") {
            continue;
        }
        let bad = |what: &str| format!("placements line {}: {what}: `{line}`", k + 1);
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() != 10 && f.len() != 11 {
            return Err(bad("expected 10 or 11 tab-separated fields"));
        }
        let cell = f[2].trim();
        let cell = u32::from_str_radix(cell.trim_start_matches("0x"), 16)
            .map_err(|_| bad("the cell is not hex"))?;
        let (block, low) = (
            ((cell >> 24) as usize, ((cell >> 16) & 0xFF) as usize),
            cell & 0xFFFF,
        );
        let has = (1..=0x40).contains(&low)
            || written
                .get(&block)
                .is_some_and(|&c| (0x0100..0x0100 + c).contains(&low));
        if !has {
            return Err(bad(&format!("cell {cell:08X} is not one this world has")));
        }
        n += 1;
    }
    Ok(n)
}

/// The world's own rules: AC2's land lies over AC1's dungeons, so Empyrean tells a dungeon by its
/// cells rather than by flat land; and the world is AC2's, so none of AC1's people, creatures,
/// doors, portals or houses from ACE's world database are placed in it.
const WORLD_RULES_TEXT: &str = "# This world's own rules, read by Empyrean.
# AC2's land lies over AC1's dungeons: a position is in a dungeon by its cell.
dungeons_by_cells = true
# The world places its own things: nothing from the world database is placed in it.
own_placements = true
";

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's build)

    /// The installs' files are held to the expected ones unless `--any-install` says otherwise;
    /// the ledger goes beside the overlay folder unless named; the start point is the tool's unless
    /// a world names its own.
    #[test]
    fn any_install_is_asked_for_and_the_ledger_is_beside_the_overlay() {
        let args = |extra: &[&str]| {
            let mut a: Vec<String> = ["--ac1", "a", "--ac2", "b", "--out", "w/overlay"]
                .iter()
                .map(|s| (*s).to_owned())
                .collect();
            a.extend(extra.iter().map(|s| (*s).to_owned()));
            Args::parse(&a)
        };
        let plain = args(&[]).expect("reads");
        assert!(!plain.any_install);
        assert_eq!(plain.ledger, Path::new("w").join("generations.tsv"));
        assert!(args(&["--any-install"]).expect("reads").any_install);
        assert_eq!(
            args(&["--ledger", "l.tsv"]).expect("reads").ledger,
            PathBuf::from("l.tsv")
        );
        assert!(args(&["--bogus", "x"]).is_err());
        // The start point is the tool's own unless a world gives its own file.
        assert_eq!(plain.start, None);
        assert_eq!(
            args(&["--start", "land/start.toml"]).expect("reads").start,
            Some(PathBuf::from("land/start.toml"))
        );
    }
    use crate::ac2_world::*;

    #[test]
    fn placements_must_stand_in_cells_the_world_has() {
        let written = std::collections::BTreeMap::from([((0x6E, 0x8F), 0x86u32)]);
        let row = |cell: &str| {
            format!(
                "0x76E8F000	24037	{cell}	1	2	3	1	0	0	0
"
            )
        };
        let header = "guid	wcid	cell	x	y	z	qw	qx	qy	qz	links
";
        assert_eq!(
            check_placements(
                &format!(
                    "{header}# a note
{}",
                    row("0x6E8F0185")
                ),
                &written
            ),
            Ok(1)
        );
        assert_eq!(check_placements(&row("0x12340040"), &written), Ok(1));
        assert!(check_placements(&row("0x6E8F0186"), &written).is_err());
        assert!(check_placements(&row("0x6E900100"), &written).is_err());
        assert!(check_placements(
            "0x76E8F000	1
",
            &written
        )
        .is_err());
    }
    /// A small area built twice gives the same bytes: AC2's ground about Cragstone read, sampled
    /// onto the dense grid, rounded into a height table fitted to it and cut, block by block.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_small_area_built_twice_gives_the_same_bytes() {
        let dir = auberean_dat::testing::require_target_install();
        let build = || -> (Vec<u8>, usize) {
            // AC2's blocks are 160 m: four of them about Cragstone hold the three AC1 blocks built.
            let (g, ..) = read_ac2_area(&dir, (0x6E, 0xA8), 4).expect("AC2's ground");
            let grid = Grid {
                blocks: 3,
                cells: 8,
                cell_len: 24.0,
                steps: STEPS,
            };
            let mut w = DenseWorld::sample(&g, grid);
            let heights: Vec<f32> = w
                .vertex
                .iter()
                .filter(|v| !v.water)
                .map(|v| v.exact)
                .collect();
            let (t, _) = table::fitted(&heights, MAX_ROUNDING);
            w.finish(&g, t);
            let mut out = Vec::new();
            for bx in 0..3 {
                for by in 0..3 {
                    let (words, h, cuts) = w.block(bx, by);
                    out.extend(words.iter().flat_map(|w| w.to_le_bytes()));
                    out.extend(h);
                    out.extend(cuts);
                }
            }
            (out, heights.len())
        };
        let (first, land) = build();
        assert!(land > 1000, "the area is land: {land} land vertices");
        assert_eq!(first, build().0);
    }
}
