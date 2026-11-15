//! Behaviour: none (tooling: the retail device's own frames kept to compare a later build with)
//!
//! The retail device's frames at seven stations, written by one build and compared by a later
//! one: a change that should leave the retail look alone (another renderer added beside it, a
//! shared crate moved) draws every one byte for byte as before. The stations are the four 2017
//! arrivals the retail comparison measures, each at its recorded moment from the chase camera's
//! place; the first of them at midnight; a town seen wide from 30 meters back, toward its stone
//! buildings and the sky; and the cave's first room from inside. Each frame is built as the
//! viewer builds it: the static object shadows, the clouds' shadows, the water's reflections and
//! the placed objects' volumes of shadow on, the materials as they stand at the client's start.
//!
//! Fixture: a hardware adapter and the end-of-retail install under `$AUBEREAN_TEST_DAT_DIR`.
//! `AUBEREAN_RETAIL_BASELINE_DIR` names the folder the capture is written to and read from.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use auberean_draw::scene::{Frame, SceneBuilder, Spot};
use auberean_draw::volume::{placed_casters, VolumeMaker, VolumeRoom, Volumes, VOLUME_REACH};
use auberean_primitives::Vec3;
use auberean_world::sky::{day_clock, day_for};
use dereth_primitives::num::math;

use crate::common::{install, renderer};

const W: u32 = 640;
const H: u32 = 480;

/// The chase camera's tilt above the look-at point, as the client's at rest: 0.25 radians.
const TILT: f32 = 0.25;

/// The field of view the client draws with at rest, vertical degrees.
const FOV: f32 = 75.0;

/// Where a station is seen from.
#[derive(Debug, Clone, Copy)]
enum Place {
    /// On the land: a point of a block (meters from its south-west corner, z the ground there),
    /// looked at from `behind` meters back along `heading` and the chase camera's tilt above, at
    /// `eye` meters over the point, at game time `time` (seconds) or, with `midnight`, at the
    /// start of that game day.
    Land {
        block: (i32, i32),
        at: [f32; 3],
        heading: f32,
        eye: f32,
        behind: f32,
        time: f64,
        midnight: bool,
    },
    /// Inside: a block's cell, looked from a point in it along `heading`, level.
    Inside {
        block: (i32, i32),
        cell: u16,
        at: [f32; 3],
        heading: f32,
    },
}

/// A station: its name and where it is seen from.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Station {
    pub(crate) name: &'static str,
    place: Place,
}

/// The stations, in order. The four arrivals' points, headings, look-at heights, distances and
/// moments are the retail comparison's.
pub(crate) const STATIONS: [Station; 7] = [
    Station {
        name: "town-6fa9",
        place: Place::Land {
            block: (0x6F, 0xA9),
            at: [45.244, 90.513, 85.255],
            heading: 177.88,
            eye: 2.369,
            behind: 5.175,
            time: 130_248_783.7,
            midnight: false,
        },
    },
    Station {
        name: "town-76b8",
        place: Place::Land {
            block: (0x76, 0xB8),
            at: [105.154, 75.968, 182.506],
            heading: 189.18,
            eye: 2.254,
            behind: 5.085,
            time: 130_146_135.5,
            midnight: false,
        },
    },
    Station {
        name: "camp-c156",
        place: Place::Land {
            block: (0xC1, 0x56),
            at: [124.842, 89.758, 79.968],
            heading: 149.65,
            eye: 2.287,
            behind: 4.995,
            time: 129_572_521.9,
            midnight: false,
        },
    },
    Station {
        name: "open-cc8b",
        place: Place::Land {
            block: (0xCC, 0x8B),
            at: [83.978, 122.348, 72.834],
            heading: 21.18,
            eye: 2.081,
            behind: 4.545,
            time: 129_416_258.8,
            midnight: false,
        },
    },
    Station {
        name: "night-6fa9",
        place: Place::Land {
            block: (0x6F, 0xA9),
            at: [45.244, 90.513, 85.255],
            heading: 177.88,
            eye: 2.369,
            behind: 5.175,
            time: 130_248_783.7,
            midnight: true,
        },
    },
    Station {
        name: "wide-6fa9",
        place: Place::Land {
            block: (0x6F, 0xA9),
            at: [45.244, 90.513, 85.255],
            heading: 0.0,
            eye: 2.369,
            behind: 30.0,
            time: 130_248_783.7,
            midnight: false,
        },
    },
    Station {
        name: "cave-6967",
        place: Place::Inside {
            block: (0x69, 0x67),
            cell: 0x0100,
            at: [109.75, 112.97, 100.8],
            heading: 0.0,
        },
    },
];

/// The station's frame, built as the viewer builds one.
pub(crate) fn frame(station: &Station) -> Frame {
    let records = &install().records;
    let region = records.region().expect("the region");
    let mut builder = SceneBuilder::new(records, &region);
    builder.static_shadows = true;
    builder.cloud_shadows = true;
    builder.water_reflections = true;
    let mut frame = match station.place {
        Place::Land {
            block,
            at,
            heading,
            eye,
            behind,
            time,
            midnight,
        } => {
            let cal = region.calendar.as_ref().expect("the region's calendar");
            let sky = region.sky.as_ref().expect("the region's sky");
            let clock = day_clock(cal, time);
            let (h, t) = (heading.to_radians(), TILT);
            let look = Vec3::new(at[0], at[1], at[2] + eye);
            let back = behind * math::cosf(t);
            let spot = Spot {
                block,
                position: Vec3::new(
                    look.x - math::sinf(h) * back,
                    look.y - math::cosf(h) * back,
                    look.z + behind * math::sinf(t),
                ),
                heading,
                pitch: -t.to_degrees(),
                time_of_day: if midnight { 0.0 } else { clock.fraction },
                day: day_for(sky, clock.year * i64::from(cal.days_per_year) + clock.day),
            };
            builder.frame(&spot, FOV)
        }
        Place::Inside {
            block,
            cell,
            at,
            heading,
        } => {
            let spot = Spot {
                block,
                position: Vec3::new(at[0], at[1], at[2]),
                heading,
                pitch: 0.0,
                time_of_day: 0.5,
                day: None,
            };
            builder.inside_frame(&spot, FOV, block, cell).0
        }
    };
    // The placed objects' volumes of shadow, as the viewer casts them.
    let inside = frame.outside.is_some();
    let sun = frame
        .outside
        .as_ref()
        .unwrap_or(&frame.light)
        .light_direction;
    frame.volumes.strength = Volumes::strength_of(frame.light.shadow_color, inside);
    let mut casters = placed_casters(&frame, sun, VOLUME_REACH);
    casters.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    let mut room = VolumeRoom::FRAME;
    let mut cast = Vec::new();
    let mut maker = VolumeMaker::default();
    for c in casters {
        maker.cast(
            records,
            &frame.objects[c.draws],
            c.toward,
            c.detail,
            builder.model_detail,
            &mut room,
            &mut cast,
        );
    }
    frame.volumes.volumes.extend(cast);
    frame.move_materials(0.0, &auberean_primitives::random::Random::new());
    frame
}

/// The station drawn by the retail device: its picture's texels.
fn draw(station: &Station) -> Vec<u8> {
    let frame = frame(station);
    let mut r = renderer();
    let records = &install().records;
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let pic = r
        .render(&frame, W, H, &mut load)
        .unwrap_or_else(|e| panic!("{}: {e}", station.name));
    assert_eq!((pic.width, pic.height), (W, H));
    pic.rgba
}

/// The digest the capture lists beside each picture: FNV-1a over its texels, enough for a person
/// reading the list to tell two frames apart.
fn digest(rgba: &[u8]) -> u64 {
    rgba.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn folder() -> Option<PathBuf> {
    std::env::var_os("AUBEREAN_RETAIL_BASELINE_DIR").map(PathBuf::from)
}

fn write_png(path: &Path, rgba: &[u8]) {
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

fn read_png(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    let mut buf = vec![0; dec.output_buffer_size()];
    let info = dec.next_frame(&mut buf).ok()?;
    ((info.width, info.height) == (W, H)).then(|| {
        buf.truncate(info.buffer_size());
        buf
    })
}

/// Not a claim: every station drawn by the retail device, written for
/// [`every_station_draws_what_the_base_build_drew`] to read in a later build: `<station>.png` and
/// `digest.txt` under the folder `AUBEREAN_RETAIL_BASELINE_DIR` names.
#[test]
#[ignore = "instrument: writes a capture of this build; set AUBEREAN_RETAIL_BASELINE_DIR and run \
            with --ignored"]
fn capture_the_retail_frames_for_a_later_build() {
    let Some(dir) = folder() else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the capture folder");
    let mut digests = String::new();
    for station in &STATIONS {
        let rgba = draw(station);
        write_png(&dir.join(format!("{}.png", station.name)), &rgba);
        let _ = writeln!(digests, "{} {:016x}", station.name, digest(&rgba));
    }
    std::fs::write(dir.join("digest.txt"), digests).expect("digest.txt");
    eprintln!("wrote {}", dir.display());
}

/// Every station drawn by this build's retail device draws the base build's frame byte for byte.
/// The base build's frames are the capture [`capture_the_retail_frames_for_a_later_build`]
/// writes.
#[test]
#[ignore = "compares with a capture of the base build: set AUBEREAN_RETAIL_BASELINE_DIR and run \
            with --ignored"]
fn every_station_draws_what_the_base_build_drew() {
    let dir = folder().expect("AUBEREAN_RETAIL_BASELINE_DIR names the base build's capture");
    let mut differ = Vec::new();
    for station in &STATIONS {
        let base = read_png(&dir.join(format!("{}.png", station.name)))
            .unwrap_or_else(|| panic!("the base capture has no frame of {}", station.name));
        let rgba = draw(station);
        let changed = rgba.iter().zip(&base).filter(|(a, b)| a != b).count();
        eprintln!("{}: {changed} bytes differ", station.name);
        if changed > 0 {
            differ.push(format!("{}: {changed} bytes", station.name));
            write_png(&dir.join(format!("{}-now.png", station.name)), &rgba);
        }
    }
    assert!(
        differ.is_empty(),
        "frames differ from the base build's: {differ:#?}"
    );
}
