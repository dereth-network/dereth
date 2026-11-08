//! The remaster with its boxes ticked as the options page ticks them, beside Retail Look: each
//! place drawn each way, the mean color of each part of the picture printed (the
//! sky, the water, the ground, what the sun lights, indoors) and the pictures written.
//!
//! A part is measured where both pictures show it: the remaster's ground goes on to the horizon and
//! retail's stops at its fog, so where the remaster has far ground retail has sky, and setting one
//! against the other says nothing of either's ground. What only the remaster draws is its own line,
//! `far ground`.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware device.
//!
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).
//! - `DERETH_REMASTER_LOOK_PLACES`: a comma list of stations and of this module's report places
//!   (default all of [`PLACES`]).
//! - `DERETH_REMASTER_LOOK_BOXES`: the boxes, as `--fidelity` takes them (default [`EVERY_BOX`]).
//! - `DERETH_REMASTER_LOOK_TAG`: a word in the remaster's pictures' names, to keep runs apart.
//! - `DERETH_REMASTER_SIZE`: `WIDTHxHEIGHT` (default 960x540).
//! - `DERETH_ART_PACK`: the art pack the remaster's pack boxes draw (default none).
//! - `DERETH_REMASTER_KNOBS`: the ground's knobs, `name = value` lines joined by `;` (default
//!   every knob at its default).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_dat::RetailDatStore;
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::RemasterCapture;

use super::remaster_frames;
use super::stations::{self, capture, report_station, Shot, Station};

/// Every box that draws something of its own without a choice of how, as a player ticks them.
pub(crate) const EVERY_BOX: &str = "Remaster=1,RemasterShadows=1,RemasterSky=1,RemasterFog=1,RemasterOcclusion=1,RemasterIndoorLamps=1,RemasterOutdoorLamps=1,RemasterBloom=1,RemasterSmoothing=1,RemasterWater=1,RemasterPuddles=1,RemasterRain=1,RemasterDetail=1,RemasterBounce=1";

/// The places of the comparison: stations by name, and report lines by theirs.
const PLACES: [&str; 8] = [
    "holtburg",
    "vista",
    "indoor",
    "dungeon",
    "lamps-street",
    "night",
    "lake-noon",
    "lake-dusk",
];

/// A lake from a hill north of Holtburg, at noon and at dusk.
const REPORTS: [(&str, &str); 2] = [
    (
        "lake-noon",
        "dereth report 1; loc 0xA9B50011 [64.387108 22.052589 38.978706] 0.920786 0.000000 \
         0.000000 -0.390069; camera 0xA9B5 [60.244957 18.04098 42.136875] yaw -0.8014121 pitch \
         -0.27999994 cell 0xA9B50011; time 0.5; clock 306486600.0; weather auto",
    ),
    (
        "lake-dusk",
        "dereth report 1; loc 0xA9B50011 [64.387108 22.052589 38.978706] 0.920786 0.000000 \
         0.000000 -0.390069; camera 0xA9B5 [60.244957 18.04098 42.136875] yaw -0.8014121 pitch \
         -0.27999994 cell 0xA9B50011; time 0.885; clock 306487158.54054123; weather auto",
    ),
];

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// A place's station and the report that holds its camera, if it is one.
fn place(store: &Arc<RetailDatStore>, name: &str) -> (Station, Option<VisualReport>) {
    if let Some((_, line)) = REPORTS.iter().find(|r| r.0 == name) {
        let report = VisualReport::parse(line).expect("the place's report reads");
        return (report_station(&report, Some(8)), Some(report));
    }
    let station = stations::stations(store)
        .into_iter()
        .chain(stations::close_stations(store))
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("no place {name}"));
    (station, None)
}

/// `boxes` drawn at the place on a device of its own on `backend`, held still until it settles:
/// the frame, and the remaster's own pictures when it drew.
fn draw(
    store: &Arc<RetailDatStore>,
    at: &(Station, Option<VisualReport>),
    backend: Backend,
    boxes: &str,
    size: (u32, u32),
) -> (Vec<u8>, Option<RemasterCapture>) {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &at.0);
    if let Some(r) = at.1.as_ref() {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    if let Some(path) = env("DERETH_ART_PACK").map(std::path::PathBuf::from) {
        let pack = Arc::new(ArtPack::open_path(&path).expect("the pack opens"));
        let folder = path.is_dir().then(|| path.clone());
        shot.scene.draw.set_remaster_pack(Some(pack), folder);
    }
    if let Some(lines) = env("DERETH_REMASTER_KNOBS") {
        let (knobs, notes) = dereth_terrain::land::drawn::Knobs::parse(&lines.replace(';', "\n"));
        assert!(notes.is_empty(), "the knobs read: {notes:?}");
        shot.scene.set_remaster_ground_knobs(knobs);
    }
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch(boxes).expect("the boxes read");
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..24 {
        shot.step_held(&mut gpu);
    }
    let rgba = capture(&mut gpu);
    let timings = shot.scene.draw.remaster_timings(&mut gpu);
    if !timings.is_empty() {
        let total: f64 = timings.iter().map(|t| t.1).sum();
        let line: Vec<String> = timings
            .iter()
            .filter(|t| t.1 >= 0.1)
            .map(|(l, ms)| format!("{l} {ms:.1}"))
            .collect();
        println!("  {total:.1} ms on the device: {}", line.join(", "));
    }
    let remaster = shot.scene.draw.remaster_capture(&mut gpu);
    drop(shot);
    (rgba, remaster)
}

/// What each pixel is on retail's own ground and geometry: the remaster at the place with every pass
/// off and retail's grid, which draws what Retail Look draws.
fn retails_parts(
    store: &Arc<RetailDatStore>,
    at: &(Station, Option<VisualReport>),
    backend: Backend,
    size: (u32, u32),
) -> RemasterCapture {
    let cfg = DeviceConfig {
        width: size.0,
        height: size.1,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(backend, None, &cfg).expect("a device");
    let mut shot = Shot::open(store, &mut gpu, &at.0);
    if let Some(r) = at.1.as_ref() {
        shot.apply_report(r);
    }
    shot.poll(&mut gpu);
    shot.step_held(&mut gpu);
    shot.scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    for _ in 0..12 {
        shot.step_held(&mut gpu);
    }
    let c = shot
        .scene
        .draw
        .remaster_capture(&mut gpu)
        .expect("the remaster drew");
    shot.scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    drop(shot);
    c
}

/// The mean color and how many pixels of a part of the picture.
#[derive(Default, Clone, Copy)]
pub(crate) struct Mean {
    sum: [u64; 3],
    n: u64,
}

impl Mean {
    fn add(&mut self, px: &[u8]) {
        for (s, c) in self.sum.iter_mut().zip(px) {
            *s += u64::from(*c);
        }
        self.n += 1;
    }

    /// The mean of each channel, in display levels.
    #[allow(clippy::cast_precision_loss)]
    pub(crate) fn levels(&self) -> [f64; 3] {
        self.sum.map(|s| s as f64 / self.n.max(1) as f64)
    }

    fn text(&self) -> String {
        if self.n == 0 {
            return "-".to_owned();
        }
        #[allow(clippy::cast_precision_loss)]
        let m = |k: usize| self.sum[k] as f64 / self.n as f64;
        format!("{:.0} {:.0} {:.0}", m(0), m(1), m(2))
    }
}

/// The parts of a picture, as the remaster's own pictures place them.
pub(crate) const PARTS: [&str; 7] = [
    "sky",
    "water",
    "ground",
    "sunlit",
    "indoor",
    "rooms",
    "far ground",
];

/// Where [`PARTS`] names what only the remaster draws: its ground where retail shows sky.
const FAR_GROUND: usize = 6;

/// A place drawn by Retail Look and by the remaster with every box ticked: each part's mean color
/// in each, the parts as the remaster's own pictures place them.
pub(crate) fn beside_retail(store: &Arc<RetailDatStore>, name: &str) -> ([Mean; 7], [Mean; 7]) {
    let at = place(store, name);
    let size = remaster_frames::size();
    let backend = crate::common::remaster_backend();
    let (retail, _) = draw(store, &at, backend, "off", size);
    let (frame, by) = draw(store, &at, backend, EVERY_BOX, size);
    let by = by.expect("the remaster drew");
    let theirs = retails_parts(store, &at, backend, size);
    (
        parts(&retail, size.0, size.1, &by, &theirs),
        parts(&frame, size.0, size.1, &by, &theirs),
    )
}

fn part_of(class: u8, kind: u8) -> Option<usize> {
    match (class, kind) {
        (_, 7) => Some(1),
        (0 | 5, _) => Some(0),
        (1, _) => Some(2),
        (2, _) => Some(3),
        (3, _) => Some(4),
        (4, _) => Some(5),
        _ => None,
    }
}

/// The mean of each part of `rgba`, over the pixels where `by` and `theirs` place the same part; the
/// ground `by` draws where `theirs` has sky is [`FAR_GROUND`].
fn parts(
    rgba: &[u8],
    width: u32,
    height: u32,
    by: &RemasterCapture,
    theirs: &RemasterCapture,
) -> [Mean; 7] {
    let mut out = [Mean::default(); 7];
    for y in 0..by.height {
        for x in 0..by.width {
            let (fx, fy) = (x + by.x, y + by.y);
            if fx >= width || fy >= height {
                continue;
            }
            let from = (y * by.width + x) as usize;
            let mine = part_of(by.class[from], by.kind[from]);
            let other = part_of(theirs.class[from], theirs.kind[from]);
            let part = match (mine, other) {
                (Some(2), Some(0)) => Some(FAR_GROUND),
                (a, b) if a == b => a,
                _ => None,
            };
            if let Some(p) = part {
                out[p].add(&rgba[((fy * width + fx) * 4) as usize..][..3]);
            }
        }
    }
    out
}

/// Each place drawn by Retail Look and by the remaster with its boxes: the parts' means printed,
/// and the pictures written under `DERETH_REMASTER_CAPTURE_DIR` when it is set.
#[test]
#[ignore = "instrument: prints the remaster's look beside Retail Look's and writes pictures; run with --ignored"]
fn capture_the_remasters_look_beside_retail() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let names: Vec<String> = env("DERETH_REMASTER_LOOK_PLACES").map_or_else(
        || PLACES.iter().map(|s| (*s).to_owned()).collect(),
        |v| v.split(',').map(|s| s.trim().to_owned()).collect(),
    );
    let boxes = env("DERETH_REMASTER_LOOK_BOXES").unwrap_or_else(|| EVERY_BOX.to_owned());
    let dir = env("DERETH_REMASTER_CAPTURE_DIR").map(std::path::PathBuf::from);
    let tag = env("DERETH_REMASTER_LOOK_TAG").unwrap_or_else(|| "remaster".to_owned());
    let size = remaster_frames::size();
    let backend = crate::common::remaster_backend();
    for name in &names {
        let at = place(&store, name);
        let (retail, _) = draw(&store, &at, backend, "off", size);
        let (frame, by) = draw(&store, &at, backend, &boxes, size);
        let by = by.expect("the remaster drew");
        let theirs = retails_parts(&store, &at, backend, size);
        let write = |what: &str, rgba: &[u8]| {
            if let Some(dir) = dir.as_ref() {
                let file = dir.join(format!("{name}-{what}.png"));
                stations::write_png(&file, size.0, size.1, rgba);
            }
        };
        write("retail", &retail);
        write(&tag, &frame);
        let of_retail = parts(&retail, size.0, size.1, &by, &theirs);
        let of_frame = parts(&frame, size.0, size.1, &by, &theirs);
        println!("{name}");
        for (k, part) in PARTS.iter().enumerate() {
            if of_frame[k].n > 0 {
                println!(
                    "  {part:10} {:7} px  retail {:11}  {tag} {}",
                    of_frame[k].n,
                    of_retail[k].text(),
                    of_frame[k].text(),
                );
            }
        }
    }
}
