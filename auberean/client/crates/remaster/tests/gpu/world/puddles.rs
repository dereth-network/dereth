//! Behaviour: none (experimental)
//!
//! The rain's puddles: flat ground out in the rain gathers them where its kind of terrain takes
//! them by the remaster's table, from a mask fixed to the world. Fixture: the daytime stations
//! outdoors, in the heaviest rain (their days are dry, so the frame's rain amount is set to 1),
//! the wet on throughout, so only the puddles differ.
//!
//! `AUBEREAN_REMASTER_CAPTURE_DIR`, when set, names a folder each station's frames wet and wet
//! with the puddles are written to.

use std::collections::BTreeMap;

use auberean_draw::scene::{Frame, GroundKind};
use auberean_remaster::render::{PuddleTable, WeatherSettings, PUDDLES};
use auberean_remaster::Remaster;

use crate::common::{frame, records, settled, turn, write_png, Station, STATIONS};

const W: u32 = 640;
const H: u32 = 480;

/// The land's class.
const GROUND: u8 = 1;

/// A table naming only kinds of terrain none of the stations shows (ice, snow and the volcanoes'),
/// and no roads.
const ELSEWHERE: PuddleTable = PuddleTable {
    kinds: &[("ICE", 3), ("highland-Snow", 3), ("VolcanoMagma", 3)],
    roads: 0,
};

fn device() -> Remaster {
    Remaster::with(&dereth_gfx::DeviceDesc {
        debug: true,
        ..dereth_gfx::DeviceDesc::default()
    })
    .expect("the remaster's device")
}

/// The first town's road, looked along from its western end at the town's moment: flat cobbles
/// from a few meters to beyond where the puddles fade.
const ROAD: Station = Station {
    name: "road-6fa9",
    block: (0x6F, 0xA9),
    at: [60.0, 95.0, 85.25],
    heading: 90.0,
    eye: 2.369,
    behind: 5.175,
    time: 130_248_783.7,
    midnight: false,
    inside: None,
};

fn station(name: &str) -> Frame {
    let s = *STATIONS
        .iter()
        .chain([&ROAD])
        .find(|s| s.name == name)
        .expect("the station");
    let mut f = frame(&s);
    f.rain = 1.0;
    f
}

/// The rain's settings: the wet on, the puddles as `puddles` says, by `grounds`.
fn rain(puddles: bool, grounds: PuddleTable) -> WeatherSettings {
    WeatherSettings {
        wet: true,
        puddles,
        grounds,
        ..WeatherSettings::default()
    }
}

/// `f` drawn settled with `settings`: the picture and the device's noise.
fn drawn(remaster: &mut Remaster, f: &Frame, settings: WeatherSettings) -> (Vec<u8>, usize) {
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    remaster.set_weather(settings);
    settled(|| remaster.render(f, W, H, &mut load).expect("the frame").rgba)
}

/// The land's pixels (class `GROUND`) that differ between `a` and `b` by more than `by` levels in
/// some channel, and how many land pixels there are.
fn land_moved(a: &[u8], b: &[u8], classes: &[u8], by: u8) -> (usize, usize) {
    let (mut n, mut moved) = (0, 0);
    for ((x, y), c) in a.chunks(4).zip(b.chunks(4)).zip(classes) {
        if *c != GROUND {
            continue;
        }
        n += 1;
        if (0..3).any(|k| x[k].abs_diff(y[k]) > by) {
            moved += 1;
        }
    }
    (moved, n)
}

/// The near land's vertices by the name of the ground they show, roads as "road".
fn grounds(f: &Frame) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for b in f.terrain.iter().filter(|b| !b.far) {
        for v in b.vertices.iter() {
            let name = match v.ground {
                GroundKind::Terrain(k) => f
                    .ground_kinds
                    .get(k as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("kind {k}")),
                GroundKind::Road => "road".to_string(),
                GroundKind::Unknown => "unknown".to_string(),
            };
            *out.entry(name).or_default() += 1;
        }
    }
    out
}

/// **Every kind the puddles' table names is one of the region's kinds of terrain**, and every near
/// layer's vertex tells its ground: so a name misspelled in the table, or a layer that lost its
/// kind, is caught.
#[test]
fn every_kind_the_puddles_table_names_is_one_of_the_regions() {
    let _turn = turn();
    let f = station("road-6fa9");
    for (name, _) in PUDDLES.kinds {
        assert!(
            f.ground_kinds.iter().any(|k| k == name),
            "the table names {name}, which the region has no kind of terrain called"
        );
    }
    let g = grounds(&f);
    eprintln!("road-6fa9's near land: {g:?}");
    assert!(!g.contains_key("unknown"), "a near layer told no ground");
}

/// **Puddles gather only in the rain, and only on the ground the table says**: in the heaviest
/// rain, puddles on the remaster's own table move some of the land's pixels by more than two levels
/// from the wet frame without them, at most one in four: along the first town's road at least two
/// thousand, and at least a hundred at the stations with a road, mud or grass near (the second
/// town, the open land's mud). With a table naming only kinds of terrain none of them shows and no
/// roads, or with no rain, the frame is the one drawn without the puddles (beyond the device's
/// noise).
#[test]
fn puddles_gather_only_in_the_rain_and_only_on_the_ground_the_table_says() {
    let _turn = turn();
    for (name, least) in [("road-6fa9", 2000), ("town-76b8", 100), ("open-cc8b", 100)] {
        let mut remaster = device();
        let f = station(name);
        eprintln!("{name}'s near land: {:?}", grounds(&f));
        let (wet, wet_noise) = drawn(&mut remaster, &f, rain(false, PUDDLES));
        let classes = remaster
            .read_classes()
            .expect("the classes")
            .expect("a frame was drawn");
        let (puddled, puddled_noise) = drawn(&mut remaster, &f, rain(true, PUDDLES));
        let (elsewhere, elsewhere_noise) = drawn(&mut remaster, &f, rain(true, ELSEWHERE));
        let mut dry_f = f.clone();
        dry_f.rain = 0.0;
        let (dry, dry_noise) = drawn(&mut remaster, &dry_f, rain(false, PUDDLES));
        let (dry_puddled, dry_puddled_noise) = drawn(&mut remaster, &dry_f, rain(true, PUDDLES));
        let noise = wet_noise
            .max(puddled_noise)
            .max(elsewhere_noise)
            .max(dry_noise)
            .max(dry_puddled_noise);
        if let Some(dir) = std::env::var_os("AUBEREAN_REMASTER_CAPTURE_DIR") {
            let dir = std::path::PathBuf::from(dir);
            write_png(&dir.join(format!("{name}-wet.png")), W, H, &wet);
            write_png(&dir.join(format!("{name}-puddles.png")), W, H, &puddled);
        }
        let (moved, n) = land_moved(&wet, &puddled, &classes, 2);
        let (strays, _) = land_moved(&wet, &elsewhere, &classes, 1);
        let (dry_moved, _) = land_moved(&dry, &dry_puddled, &classes, 0);
        eprintln!(
            "{name}: of {n} land pixels the puddles moved {moved} by over 2; the table of other \
             kinds {strays} by over 1; with no rain {dry_moved} at all (noise {noise})"
        );
        assert!(moved >= least, "{name}: the puddles moved {moved} pixels");
        assert!(moved * 4 <= n, "{name}: the puddles moved {moved} of {n}");
        assert!(
            strays <= noise,
            "{name}: puddles gathered on kinds the table didn't name ({strays} pixels)"
        );
        assert!(
            noise > 0 || dry_moved == 0,
            "{name}: puddles gathered with no rain ({dry_moved} pixels)"
        );
        let errors = dereth_gfx::take_validation_errors();
        assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    }
}

/// **The puddles are fixed to the world**: along the road in the heaviest rain, the puddles drawn
/// with the frame's origin moved, as a viewer standing in another block draws the same place,
/// lie elsewhere (at least a hundred of the land's pixels the puddles move at one origin and not
/// the other), and at its own origin again lie where they lay (beyond the device's noise); and
/// the frame drawn again is the same, so they hold still from frame to frame.
#[test]
fn the_puddles_are_fixed_to_the_world() {
    let _turn = turn();
    let mut remaster = device();
    let f = station("road-6fa9");
    let mut moved_f = f.clone();
    moved_f.origin = [f.origin[0] + 5.5, f.origin[1] + 3.25];
    let (wet, wet_noise) = drawn(&mut remaster, &f, rain(false, PUDDLES));
    let classes = remaster
        .read_classes()
        .expect("the classes")
        .expect("a frame was drawn");
    let (own, own_noise) = drawn(&mut remaster, &f, rain(true, PUDDLES));
    let (moved_wet, moved_wet_noise) = drawn(&mut remaster, &moved_f, rain(false, PUDDLES));
    let (shifted, shifted_noise) = drawn(&mut remaster, &moved_f, rain(true, PUDDLES));
    let (again, again_noise) = drawn(&mut remaster, &f, rain(true, PUDDLES));
    let noise = [
        wet_noise,
        own_noise,
        moved_wet_noise,
        shifted_noise,
        again_noise,
    ]
    .into_iter()
    .max()
    .unwrap_or(0);
    // Where the puddles lie at each origin: the land's pixels they move from the wet frame there.
    let lying = |wet: &[u8], puddled: &[u8]| -> Vec<bool> {
        wet.chunks(4)
            .zip(puddled.chunks(4))
            .zip(&classes)
            .map(|((x, y), c)| *c == GROUND && (0..3).any(|k| x[k].abs_diff(y[k]) > 2))
            .collect()
    };
    let here = lying(&wet, &own);
    let there = lying(&moved_wet, &shifted);
    let elsewhere = here.iter().zip(&there).filter(|(a, b)| a != b).count();
    let (back, _) = land_moved(&own, &again, &classes, 1);
    eprintln!(
        "road-6fa9: {} land pixels puddled at its origin, {} at the moved one, {elsewhere} at one \
         and not the other; at its own again {back} moved (noise {noise})",
        here.iter().filter(|p| **p).count(),
        there.iter().filter(|p| **p).count(),
    );
    assert!(
        elsewhere >= 100,
        "the puddles moved with the origin in only {elsewhere} pixels"
    );
    assert!(
        back <= noise,
        "the puddles at its own origin moved {back} pixels"
    );
    let errors = dereth_gfx::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
}
