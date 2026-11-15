//! The target install's records, opened once per test binary; the stations and their frames; and
//! the devices to draw them on, one test at a time.

use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use auberean_assets::Context;
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::require_target_install;
use auberean_dat::DatFile;
use auberean_draw::scene::{Frame, SceneBuilder, Spot};
use auberean_primitives::Vec3;
use auberean_world::records::WorldRecords;
use auberean_world::sky::{day_clock, day_for};
use dereth_primitives::num::math;

/// The records of the target install; the test fails when the install is not found.
pub fn records() -> &'static WorldRecords {
    static RECORDS: OnceLock<WorldRecords> = OnceLock::new();
    RECORDS.get_or_init(|| {
        let dir = require_target_install();
        let mut files: Vec<(DatName, DatFile)> = install_dats(&dir)
            .expect("list the install")
            .into_iter()
            .map(|(n, p)| {
                (
                    n,
                    DatFile::open(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display())),
                )
            })
            .collect();
        let mut take = |name: &DatName| {
            let k = files.iter().position(|(n, _)| n == name)?;
            Some(files.swap_remove(k).1)
        };
        let portal = take(&DatName::Portal).expect("a portal file");
        let cell = take(&DatName::Cell(1)).expect("a cell file");
        let highres = take(&DatName::HighRes);
        let ctx = Context::from_portal(&portal).expect("the property list");
        WorldRecords::new(portal, cell, highres, ctx)
    })
}

/// One test at a time has the machine's device: a device made while another test draws on its
/// own has stalled the driver.
pub fn turn() -> MutexGuard<'static, ()> {
    static TURN: Mutex<()> = Mutex::new(());
    TURN.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The chase camera's tilt above the look-at point, as the client's at rest: 0.25 radians.
const TILT: f32 = 0.25;

/// The field of view the client draws with at rest, vertical degrees.
pub const FOV: f32 = 75.0;

/// A station: a point of a block (meters from its south-west corner, z the ground there), looked
/// at from `behind` meters back along `heading` and the chase camera's tilt above, at `eye` meters
/// over the point, at game time `time` (seconds), or at the start of that game day with
/// `midnight`; or, with `inside`, from that point itself in that cell of the block, level.
#[derive(Debug, Clone, Copy)]
pub struct Station {
    pub name: &'static str,
    pub block: (i32, i32),
    pub at: [f32; 3],
    pub heading: f32,
    pub eye: f32,
    pub behind: f32,
    pub time: f64,
    pub midnight: bool,
    pub inside: Option<u16>,
}

/// The four 2017 arrivals the retail comparison measures, at their recorded moments, from the
/// chase camera's place; the first of them at midnight; a town seen wide from 30 meters back,
/// toward its stone buildings and the sky; and the cave's first room from inside.
pub const STATIONS: [Station; 7] = [
    Station {
        name: "town-6fa9",
        block: (0x6F, 0xA9),
        at: [45.244, 90.513, 85.255],
        heading: 177.88,
        eye: 2.369,
        behind: 5.175,
        time: 130_248_783.7,
        midnight: false,
        inside: None,
    },
    Station {
        name: "town-76b8",
        block: (0x76, 0xB8),
        at: [105.154, 75.968, 182.506],
        heading: 189.18,
        eye: 2.254,
        behind: 5.085,
        time: 130_146_135.5,
        midnight: false,
        inside: None,
    },
    Station {
        name: "camp-c156",
        block: (0xC1, 0x56),
        at: [124.842, 89.758, 79.968],
        heading: 149.65,
        eye: 2.287,
        behind: 4.995,
        time: 129_572_521.9,
        midnight: false,
        inside: None,
    },
    Station {
        name: "open-cc8b",
        block: (0xCC, 0x8B),
        at: [83.978, 122.348, 72.834],
        heading: 21.18,
        eye: 2.081,
        behind: 4.545,
        time: 129_416_258.8,
        midnight: false,
        inside: None,
    },
    Station {
        name: "night-6fa9",
        block: (0x6F, 0xA9),
        at: [45.244, 90.513, 85.255],
        heading: 177.88,
        eye: 2.369,
        behind: 5.175,
        time: 130_248_783.7,
        midnight: true,
        inside: None,
    },
    Station {
        name: "wide-6fa9",
        block: (0x6F, 0xA9),
        at: [45.244, 90.513, 85.255],
        heading: 0.0,
        eye: 2.369,
        behind: 30.0,
        time: 130_248_783.7,
        midnight: false,
        inside: None,
    },
    Station {
        name: "cave-6967",
        block: (0x69, 0x67),
        at: [109.75, 112.97, 100.8],
        heading: 0.0,
        eye: 0.0,
        behind: 0.0,
        time: 0.0,
        midnight: false,
        inside: Some(0x0100),
    },
];

/// The station's frame, built as the viewer builds one (the static object shadows, the clouds'
/// shadows and the water's reflections on), the materials as they stand at the client's start.
pub fn frame(s: &Station) -> Frame {
    let records = records();
    let region = records.region().expect("the region");
    let mut builder = SceneBuilder::new(records, &region);
    builder.static_shadows = true;
    builder.cloud_shadows = true;
    builder.water_reflections = true;
    if let Some(cell) = s.inside {
        let spot = Spot {
            block: s.block,
            position: Vec3::new(s.at[0], s.at[1], s.at[2]),
            heading: s.heading,
            pitch: 0.0,
            time_of_day: 0.5,
            day: None,
        };
        let mut frame = builder.inside_frame(&spot, FOV, s.block, cell).0;
        frame.move_materials(0.0, &auberean_primitives::random::Random::new());
        return frame;
    }
    let cal = region.calendar.as_ref().expect("the region's calendar");
    let sky = region.sky.as_ref().expect("the region's sky");
    let clock = day_clock(cal, s.time);
    let h = s.heading.to_radians();
    let look = Vec3::new(s.at[0], s.at[1], s.at[2] + s.eye);
    let back = s.behind * math::cosf(TILT);
    let spot = Spot {
        block: s.block,
        position: Vec3::new(
            look.x - math::sinf(h) * back,
            look.y - math::cosf(h) * back,
            look.z + s.behind * math::sinf(TILT),
        ),
        heading: s.heading,
        pitch: -TILT.to_degrees(),
        time_of_day: if s.midnight { 0.0 } else { clock.fraction },
        day: day_for(sky, clock.year * i64::from(cal.days_per_year) + clock.day),
    };
    let mut frame = builder.frame(&spot, FOV);
    frame.move_materials(0.0, &auberean_primitives::random::Random::new());
    frame
}

/// Write `rgba` (`width` x `height`) as a PNG at `path`.
pub fn write_png(path: &std::path::Path, width: u32, height: u32, rgba: &[u8]) {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).expect("the picture's folder");
    }
    let file = std::fs::File::create(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
