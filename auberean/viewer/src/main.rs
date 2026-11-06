//! The offline world viewer: draws the world at a chosen spot and time of day to a picture, with no
//! network.
//!
//! **Depends on** the world (`auberean-world`), its records (`auberean-dat`, `auberean-assets`),
//! bodies (`auberean-animation`, `auberean-physics`), drawing (`auberean-draw`) and the device
//! (`auberean-gpu`), with `auberean-primitives` and `dereth-primitives`. **Used by** nothing: it is
//! the `auberean-viewer` program.
//!
//! ```text
//! auberean-viewer --install DIR --block 6FA9 --at 45.2,90.5,85.3 --heading 178 --time 0.4 --out FILE.png
//!            [--size 1280x960] [--fov 75] [--behind 4.5] [--tilt 14.32] [--eye 2.06] [--day 0x1A000001]
//!            [--game-time SECONDS] [--overlay-dat-dir DIR]
//! ```
//!
//! `--walk SECONDS` puts the player's body at `--at` instead, walks it (standing half a second,
//! then running; `--turn-at S` turns left for a second from S, `--jump-at S` jumps at S) and writes
//! `--frames N` frames from the chase camera, and a strip of them, into the folder `--out`.
//!
//! `--uptime SECONDS` is how long the client has been running: the clouds' shadows scroll with it
//! (default 0). `--no-shadows`, `--no-clouds` and `--no-reflections` turn the static object
//! shadows, the cloud shadows and the water's reflections off, as their preferences do;
//! `--no-volumes` the volumes of shadow (the walking body's and the placed objects' that cast
//! one), as the dynamic object shadows preference does; `--disc` lays the simple setting's dark disc under it instead. `--no-pools` leaves out
//! the pools of light the placed objects' effect lights lay on the land.
//!
//! `--fx TABLE:EFFECT@X,Y,Z[@SECONDS]` plays an effect at a point of the block's frame for
//! `SECONDS` (default 1) before the frame, and `--ps ID@X,Y,Z[@SECONDS]` one particle system
//! (both may repeat). The objects placed with the world play their looks' startup effects (fires,
//! glows, gateways' swirls) for `--startup SECONDS` (default 5) before the frame.
//!
//! `--game-time` takes the moment from a game clock in seconds (a recorded server time): the
//! calendar gives the day fraction and the sky's day for that date, overriding `--time` and
//! `--day`.
//!
//! `--at` is a point in the block's frame (meters from its south-west corner; z the ground there,
//! as a recorded position gives it). The camera looks at the point `--eye` meters above it from
//! `--behind` meters back along the heading and `--tilt` degrees above, as the client's chase
//! camera does; `--behind 0` looks from that point itself. The defaults are the client's at rest:
//! a vertical field of view of 75 degrees, a tilt of 0.25 radians, 4.5 meters back, and a look-at
//! point at the height of a character body of scale 1 (a character's height scale multiplies both
//! `--eye` and `--behind`). Draws on the hardware adapter only.
//!
//! **Must never** reach a network or a server: it reads an install and writes pictures.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::overlay::WorldOverlay;
use auberean_dat::DatFile;
use auberean_primitives::{DataId, Vec3};
use auberean_world::records::WorldRecords;
use dereth_primitives::num::math;

mod fx;
mod walk;

#[derive(Debug)]
struct Args {
    install: PathBuf,
    overlay: Option<PathBuf>,
    block: (i32, i32),
    at: Vec3,
    heading: f32,
    time: f32,
    game_time: Option<f64>,
    day: Option<u32>,
    size: (u32, u32),
    fov: f32,
    behind: f32,
    tilt: f32,
    eye: f32,
    out: PathBuf,
    walk: Option<f64>,
    frames: u32,
    turn_at: Option<f64>,
    jump_at: Option<f64>,
    uptime: f64,
    fx: Vec<fx::Placed>,
    startup: f64,
    shadows: bool,
    clouds: bool,
    reflections: bool,
    volumes: bool,
    disc: bool,
    pools: bool,
}

fn parse() -> Result<Args, String> {
    let mut a = Args {
        install: PathBuf::new(),
        overlay: None,
        block: (0, 0),
        at: Vec3::ZERO,
        heading: 0.0,
        time: 0.4,
        game_time: None,
        day: None,
        size: (1280, 960),
        fov: 75.0,
        behind: 4.5,
        tilt: 14.323_945,
        eye: 2.06,
        out: PathBuf::from("frame.png"),
        walk: None,
        frames: 8,
        turn_at: None,
        jump_at: None,
        uptime: 0.0,
        fx: Vec::new(),
        startup: 5.0,
        shadows: true,
        clouds: true,
        reflections: true,
        volumes: true,
        disc: false,
        pools: true,
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let mut v = || it.next().ok_or_else(|| format!("{k} needs a value"));
        let num = |s: String| s.parse::<f32>().map_err(|e| format!("{s}: {e}"));
        match k.as_str() {
            "--install" => a.install = PathBuf::from(v()?),
            "--overlay-dat-dir" => a.overlay = Some(PathBuf::from(v()?)),
            "--block" => {
                let s = v()?;
                let n = u32::from_str_radix(s.trim_start_matches("0x"), 16)
                    .map_err(|e| format!("{s}: {e}"))?;
                let n = if n > 0xFFFF { n >> 16 } else { n };
                a.block = (
                    i32::try_from(n >> 8).unwrap_or(0),
                    i32::try_from(n & 0xFF).unwrap_or(0),
                );
            }
            "--at" => {
                let s = v()?;
                let p: Vec<f32> = s
                    .split(',')
                    .map(|x| x.trim().parse::<f32>())
                    .collect::<Result<_, _>>()
                    .map_err(|e| format!("{s}: {e}"))?;
                a.at = Vec3::new(
                    p.first().copied().unwrap_or(0.0),
                    p.get(1).copied().unwrap_or(0.0),
                    p.get(2).copied().unwrap_or(0.0),
                );
            }
            "--heading" => a.heading = num(v()?)?,
            "--time" => a.time = num(v()?)?,
            "--game-time" => {
                let s = v()?;
                a.game_time = Some(s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?);
            }
            "--day" => {
                let s = v()?;
                a.day = Some(
                    u32::from_str_radix(s.trim_start_matches("0x"), 16)
                        .map_err(|e| format!("{s}: {e}"))?,
                );
            }
            "--size" => {
                let s = v()?;
                let (w, h) = s.split_once('x').ok_or_else(|| format!("{s}: want WxH"))?;
                a.size = (
                    w.parse().map_err(|e| format!("{s}: {e}"))?,
                    h.parse().map_err(|e| format!("{s}: {e}"))?,
                );
            }
            "--fov" => a.fov = num(v()?)?,
            "--behind" => a.behind = num(v()?)?,
            "--tilt" => a.tilt = num(v()?)?,
            "--eye" => a.eye = num(v()?)?,
            "--out" => a.out = PathBuf::from(v()?),
            "--walk" => {
                let s = v()?;
                a.walk = Some(s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?);
            }
            "--frames" => {
                let s = v()?;
                a.frames = s.parse::<u32>().map_err(|e| format!("{s}: {e}"))?;
            }
            "--turn-at" => {
                let s = v()?;
                a.turn_at = Some(s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?);
            }
            "--jump-at" => {
                let s = v()?;
                a.jump_at = Some(s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?);
            }
            "--fx" => a.fx.push(fx::parse(&v()?, false)?),
            "--ps" => a.fx.push(fx::parse(&v()?, true)?),
            "--startup" => a.startup = f64::from(num(v()?)?),
            "--uptime" => {
                let s = v()?;
                a.uptime = s.parse::<f64>().map_err(|e| format!("{s}: {e}"))?;
            }
            "--no-shadows" => a.shadows = false,
            "--no-clouds" => a.clouds = false,
            "--no-reflections" => a.reflections = false,
            "--no-volumes" => a.volumes = false,
            "--no-pools" => a.pools = false,
            "--disc" => {
                a.disc = true;
                a.volumes = false;
            }
            "-h" | "--help" => return Err(String::from("see the usage at the top of src/main.rs")),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if a.install.as_os_str().is_empty() {
        return Err(String::from("--install DIR is required"));
    }
    Ok(a)
}

fn open(install: &Path, overlay: Option<&Path>) -> Result<WorldRecords, String> {
    let overlay = overlay
        .map(WorldOverlay::open)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut files: Vec<(DatName, DatFile)> = Vec::new();
    for (n, p) in install_dats(install).map_err(|e| format!("{}: {e}", install.display()))? {
        let Ok(d) = DatFile::open(&p) else {
            continue;
        };
        let d = match &overlay {
            Some(w) => w.over(d).map_err(|e| e.to_string())?,
            None => d,
        };
        files.push((n, d));
    }
    let mut take = |name: &DatName| {
        let k = files.iter().position(|(n, _)| n == name)?;
        Some(files.swap_remove(k).1)
    };
    let portal = take(&DatName::Portal).ok_or("no portal file")?;
    let cell = take(&DatName::Cell(1)).ok_or("no cell file")?;
    let highres = take(&DatName::HighRes);
    let language = take(&DatName::Local("English".to_owned()));
    let ctx = Context::from_portal(&portal).map_err(|e| format!("{e:?}"))?;
    let records = WorldRecords::new(portal, cell, highres, ctx);
    if let Some(file) = language {
        records.use_language_file(file);
    }
    Ok(records)
}

/// The volumes of shadow the objects the world places cast in `frame` (those its builder marked
/// that stand within reach of its camera, nearest first), added to its volumes along the sun's
/// light outdoors, with the frame's shadow strength.
fn placed_volumes(
    frame: &mut auberean_draw::scene::Frame,
    records: &WorldRecords,
    model_detail: u32,
    maker: &mut auberean_draw::volume::VolumeMaker,
) {
    use auberean_draw::volume::{placed_casters, VolumeRoom, Volumes};
    let inside = frame.outside.is_some();
    let sun = frame
        .outside
        .as_ref()
        .unwrap_or(&frame.light)
        .light_direction;
    frame.volumes.strength = Volumes::strength_of(frame.light.shadow_color, inside);
    let mut casters = placed_casters(frame, sun, auberean_draw::volume::VOLUME_REACH);
    casters.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    let mut room = VolumeRoom::FRAME;
    let mut out = Vec::new();
    for c in casters {
        maker.cast(
            records,
            &frame.objects[c.draws],
            c.toward,
            c.detail,
            model_detail,
            &mut room,
            &mut out,
        );
    }
    frame.volumes.volumes.extend(out);
}

fn write_png(path: &PathBuf, pic: &auberean_gpu::Picture) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), pic.width, pic.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(&pic.rgba).map_err(|e| e.to_string())
}

fn run() -> Result<(), String> {
    let a = parse()?;
    let records = open(&a.install, a.overlay.as_deref())?;
    let region = records.region().map_err(|e| format!("{e:?}"))?;
    if a.walk.is_some() {
        return walk::run(&a, &records, &region);
    }
    let (h, t) = (a.heading.to_radians(), a.tilt.to_radians());
    let eye = Vec3::new(a.at.x, a.at.y, a.at.z + a.eye);
    let back = a.behind * math::cosf(t);
    let position = Vec3::new(
        eye.x - math::sinf(h) * back,
        eye.y - math::cosf(h) * back,
        eye.z + a.behind * math::sinf(t),
    );
    let spot = auberean_draw::scene::Spot {
        block: a.block,
        position,
        heading: a.heading,
        pitch: if a.behind > 0.0 { -a.tilt } else { 0.0 },
        time_of_day: a.time,
        day: a.day.map(DataId),
    };
    let mut spot = spot;
    if let (Some(t), Some(cal), Some(sky)) =
        (a.game_time, region.calendar.as_ref(), region.sky.as_ref())
    {
        let c = auberean_world::sky::day_clock(cal, t);
        spot.time_of_day = c.fraction;
        spot.day = auberean_world::sky::day_for(sky, c.year * i64::from(cal.days_per_year) + c.day);
        println!(
            "game time {t}: year {} day {} fraction {:.4}, sky day {:08X}",
            c.year,
            c.day,
            c.fraction,
            spot.day.map_or(0, |d| d.0)
        );
    }
    let mut builder = auberean_draw::scene::SceneBuilder::new(&records, &region);
    builder.uptime = a.uptime;
    builder.static_shadows = a.shadows;
    builder.cloud_shadows = a.clouds;
    builder.water_reflections = a.reflections;
    let mut frame = builder.frame(&spot, a.fov);
    if a.volumes {
        placed_volumes(
            &mut frame,
            &records,
            builder.model_detail,
            &mut auberean_draw::volume::VolumeMaker::default(),
        );
    }
    fx::add(&mut frame, &records, &a.fx);
    let pools = a.pools.then_some((&builder, spot.time_of_day));
    fx::add_placed(&mut frame, &records, a.block, a.startup, pools);
    // The materials as they stand at the running time asked for.
    frame.move_materials(a.uptime, &auberean_primitives::random::Random::new());
    let mut renderer = auberean_gpu::Renderer::new().map_err(|e| e.to_string())?;
    let mut load = |id: u32| auberean_draw::picture::load(&records, id);
    let pic = renderer
        .render(&frame, a.size.0, a.size.1, &mut load)
        .map_err(|e| e.to_string())?;
    write_png(&a.out, &pic)?;
    let c = frame.counts;
    println!(
        "{}: block {:02X}{:02X} at ({:.1}, {:.1}, {:.1}) heading {} time {:.3} on {}; {} blocks, {} squares ({} holes), {} objects, {} parts, {} cells, {} scenery ({} frills), {} sky objects, {} meshes missing; light dir ({:.2}, {:.2}, {:.2}) fog {:.0}..{:.0}",
        a.out.display(),
        a.block.0,
        a.block.1,
        a.at.x,
        a.at.y,
        a.at.z,
        a.heading,
        spot.time_of_day,
        renderer.adapter,
        c.blocks,
        c.squares,
        c.holes,
        c.objects,
        c.parts,
        c.cells,
        c.scenery,
        c.frills,
        c.sky_objects,
        c.missing_meshes,
        frame.light.light_direction.x,
        frame.light.light_direction.y,
        frame.light.light_direction.z,
        frame.light.fog_min,
        frame.light.fog_max,
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("auberean-viewer: {e}");
            ExitCode::FAILURE
        }
    }
}
