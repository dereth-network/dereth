//! The map window from the September 2013 install: its picture drawn from the world's landblocks,
//! the closer maps' labels written from the game's strings, the fellows' marker, and the game's
//! date and time.
//!
//! `AUBEREAN_MAP_SHOT_DIR`, when set, names a folder the map's picture of the whole is written to,
//! to look at.

use std::sync::Arc;

use auberean_horizon::art::Records;
use auberean_horizon::atlas::{date_and_time, fellow_picture, read};
use auberean_horizon::world_map::{laid_on, MapBuild, MapLand, OVERVIEW, SQUARE_PX};
use auberean_world::terrain::build_block;

/// Behaviour: desktop.horizon.the-map-reads-the-world-s-size-and-labels-from-the-region
#[test]
fn the_map_reads_the_worlds_size_and_the_closer_maps_labels_from_the_region() {
    let (world, _, assets) = crate::session::data();
    let records: Records = Arc::clone(assets) as _;
    let atlas = read(world, Some(&records));
    assert_eq!(atlas.land, (2040, 2040), "255 blocks a side");
    assert!(!atlas.labels.is_empty());
    let written = atlas.labels.iter().filter(|l| l.words.is_some()).count();
    assert!(
        written * 2 > atlas.labels.len(),
        "{written} of {} written",
        atlas.labels.len()
    );
    for l in &atlas.labels {
        assert!(
            (0.0..2040.0).contains(&l.at.0) && (0.0..2040.0).contains(&l.at.1),
            "{:?} on the land",
            l.at
        );
    }
    // The continents, as the game's map window names them, each where its world map writes it.
    let named: Vec<&str> = atlas.continents.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(
        named,
        ["Osteth", "Omishan", "Linvak Massif", "Arramora", "Knorr"],
        "{:?}",
        atlas.continents
    );
    let at = |n: &str| {
        atlas
            .continents
            .iter()
            .find(|c| c.name == n)
            .expect("named")
            .at
    };
    for c in &atlas.continents {
        assert!(
            (0.0..=2040.0).contains(&c.at.0) && (0.0..=2040.0).contains(&c.at.1),
            "{c:?} on the land"
        );
    }
    assert!(
        at("Osteth").1 > at("Linvak Massif").1,
        "Osteth north of Linvak Massif"
    );
    assert!(at("Osteth").1 > at("Knorr").1, "and of Knorr");
    assert!(
        at("Omishan").0 > at("Arramora").0,
        "Omishan east of Arramora"
    );
    assert!(
        at("Knorr").0 > at("Linvak Massif").0,
        "Knorr east of Linvak Massif"
    );
    let fellow = fellow_picture(&records);
    assert!(
        records.image(fellow).is_some(),
        "the fellows' marker {fellow:08x}"
    );
    let (date, time) = date_and_time(world, &records, 1.0e6).expect("the calendar");
    assert!(
        !date.trim().is_empty() && !time.trim().is_empty(),
        "{date:?} {time:?}"
    );
}

/// The map's time of day and date at a fixed game time, from the region's calendar and the game's
/// strings, as the game's own map window writes them.
/// Behaviour: desktop.horizon.the-map-writes-the-time-of-day-and-the-date-from-the-calendar
#[test]
fn the_map_writes_the_time_of_day_and_the_date_from_the_calendar() {
    let (world, _, assets) = crate::session::data();
    let records: Records = Arc::clone(assets) as _;
    // Late on the 21st day of Leafcull in the year 664.
    let (date, time) = date_and_time(world, &records, 440_748_000.0).expect("the calendar");
    assert_eq!(time, "Sunset");
    assert_eq!(date, "Leafcull 21, 664 P.Y.");
}

/// The map's picture drawn from every landblock off the frame: a pixel to each square of the
/// land, its picture of the whole shrunk to 2048 a side; each block laid in its far landscape's
/// colors as the ground picks them, the land drawn in them and the sea about it.
/// Behaviour: desktop.horizon.the-map-draws-the-world-from-its-landblocks
#[test]
fn the_map_draws_the_world_from_its_landblocks_in_the_far_landscapes_colors() {
    let (world, _, _) = crate::session::data();
    let land = Arc::new(MapLand::of(&world.region));
    assert_eq!(land.blocks, (255, 255));
    let built = MapBuild::start(world.records.landblock_reader(), Arc::clone(&land), true)
        .wait()
        .expect("the map builds");
    assert_eq!(
        built.map.size(),
        (255 * 16 * SQUARE_PX, 255 * 16 * SQUARE_PX)
    );
    let whole = built.overview.expect("the picture of the whole");
    let side = u32::try_from(OVERVIEW).expect("a side");
    assert_eq!((whole.width, whole.height), (side, side));
    // A block of the land is laid in the far colors the ground picks for it.
    let records = &world.records;
    let mut blocks = std::collections::HashMap::new();
    let (bx, by) = (0x29, 0x98);
    for x in bx - 1..=bx + 1 {
        for y in by - 1..=by + 1 {
            if let Some(lb) = records.landblock(x, y) {
                blocks.insert((x, y), lb);
            }
        }
    }
    let ground = build_block(&world.region.land, world.region.surfaces(), &blocks, bx, by)
        .expect("the block is land");
    let laid = land.lay(&blocks, bx, by).expect("laid");
    for (k, c) in laid.colors.iter().enumerate() {
        let far = ground.terrain[k].far_color;
        let want = [far.r, far.g, far.b].map(|v| (v.clamp(0.0, 1.0) * 255.0).round());
        let got = c.map(f32::from);
        assert_eq!(got, want, "vertex {k}");
    }
    assert!(
        laid.colors.iter().any(|c| *c != [255, 255, 255]),
        "colored, not white"
    );
    // The roads are drawn in the roads' color.
    let (cols, rows) = built.map.tiles();
    let mut road = 0usize;
    for col in 0..cols {
        for row in 0..rows {
            let tile = built.map.tile((col, row));
            road += tile
                .bgra
                .chunks(4)
                .filter(|p| p[..3] == [167, 181, 186])
                .count();
        }
    }
    assert!(road > 1_000, "{road} pixels of road");
    // The land is many colors; the open sea is the sea's.
    let colors: std::collections::HashSet<&[u8]> = whole.bgra.chunks(4).collect();
    assert!(colors.len() > 5_000, "{} colors", colors.len());
    let [r, g, b] = auberean_horizon::world_map::sea_color();
    assert_eq!(
        &whole.bgra[..3],
        &[b, g, r],
        "the north-west corner is open sea"
    );
    for m in built.map.landmasses().iter().take(9) {
        eprintln!("MASS {m:?}");
    }
    // The continents named on the land: each in the middle of its own stretch of it, the five
    // largest, the largest Osteth's.
    let records: Records = Arc::clone(&crate::session::data().2) as _;
    let atlas = read(world, Some(&records));
    let masses = built.map.landmasses();
    let placed = auberean_horizon::atlas::on_the_land(&atlas.continents, masses, atlas.land);
    let on = |name: &str| {
        let c = placed.iter().find(|c| c.name == name).expect("named");
        let m = masses
            .iter()
            .position(|m| {
                (m.centre.0 * 2040.0 - c.at.0).abs() < 1e-2
                    && (m.centre.1 * 2040.0 - c.at.1).abs() < 1e-2
            })
            .expect("on a stretch of land");
        m
    };
    let mut which: Vec<usize> = ["Osteth", "Omishan", "Linvak Massif", "Knorr", "Arramora"]
        .iter()
        .map(|n| on(n))
        .collect();
    assert_eq!(which[0], 0, "Osteth the largest");
    which.sort_unstable();
    assert_eq!(which, [0, 1, 2, 3, 4], "each its own of the five largest");
    keep("map-overview", &whole);
}

/// The placeholder land left out of the map: over every landblock of the world, the stretches of
/// land that copy another are exactly nine, of 62 landblocks each, all north of the continents;
/// the specks of flat ground far from other land are exactly two, at the south-west corner and
/// out in the north-east; every other stretch, the continents and their islands, keeps all its
/// land.
/// Behaviour: desktop.horizon.the-map-leaves-out-the-placeholder-islands
#[test]
fn the_map_leaves_out_the_nine_placeholder_islands_and_two_specks_and_no_other_land() {
    use auberean_horizon::world_map::Why;
    let (land, mut grid) = laid();
    let before = grid.landmasses();
    let stretches = grid.stretches();
    let continents_north = {
        let mut by_size = stretches.clone();
        by_size.sort_by_key(|s| std::cmp::Reverse(s.len()));
        by_size[..5]
            .iter()
            .flatten()
            .map(|k| k / land.blocks.0)
            .max()
            .expect("the continents")
    };
    let dropped = grid.drop_placeholder_land();
    let copies: Vec<_> = dropped.iter().filter(|p| p.why == Why::Copy).collect();
    assert_eq!(copies.len(), 9, "{dropped:?}");
    for p in &copies {
        assert_eq!(p.blocks, 62, "{p:?}");
        assert!(
            p.south_west.1 > continents_north,
            "{p:?} north of the continents"
        );
    }
    let specks: Vec<_> = dropped
        .iter()
        .filter(|p| p.why == Why::Speck)
        .map(|p| (p.blocks, p.south_west, p.north_east))
        .collect();
    assert_eq!(
        specks,
        [(4, (0, 0), (1, 1)), (1, (248, 248), (248, 248))],
        "{dropped:?}"
    );
    // Every other stretch is as it was.
    let after = grid.landmasses();
    assert_eq!(after.len(), before.len() - 11);
    let mut gone = before.clone();
    for m in &after {
        let k = gone.iter().position(|b| b == m).expect("kept as it was");
        gone.remove(k);
    }
    let mut sizes: Vec<usize> = gone.iter().map(|m| m.blocks).collect();
    sizes.sort_unstable();
    assert_eq!(
        sizes,
        [1, 4, 62, 62, 62, 62, 62, 62, 62, 62, 62],
        "{gone:?}"
    );
    assert_eq!(
        &after[..5],
        &before[..5],
        "the continents keep all their land"
    );
}

/// The land the map frames fully zoomed out, the placeholder land left out: the continents and
/// their islands, from landblock 43 across to 250 and 32 up to 205; outside them only the nine
/// copies and the two specks, which are left out.
/// Behaviour: desktop.horizon.the-map-frames-the-land
#[test]
fn the_map_frames_the_continents_and_their_islands() {
    let (_, mut grid) = laid();
    let everything = grid.stretches();
    grid.drop_placeholder_land();
    let b = grid.land_bounds().expect("land");
    let blocks = |v: f32| (v * 255.0).round();
    assert_eq!(
        [b.west, b.south, b.east, b.north].map(blocks),
        [43.0, 32.0, 251.0, 206.0],
        "{b:?}"
    );
    let outside: Vec<_> = everything
        .iter()
        .filter(|s| {
            s.iter().any(|k| {
                let (x, y) = (k % 255, k / 255);
                !(43..251).contains(&x) || !(32..206).contains(&y)
            })
        })
        .map(|s| (s.len(), s[0] % 255, s[0] / 255))
        .collect();
    assert_eq!(
        outside.len(),
        2 + 9,
        "the two specks and the nine copies: {outside:?}"
    );
}

/// Every landblock of the install's world laid on the map's grid as the map's build lays them.
fn laid() -> (MapLand, auberean_horizon::world_map::MapGrid) {
    let (world, _, _) = crate::session::data();
    let land = MapLand::of(&world.region);
    let machine = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let grid = laid_on(&world.records.landblock_reader(), &land, machine);
    (land, grid)
}

/// The map's picture is the same however many threads read the landblocks: on one, two or every
/// thread the machine has, the picture at its own size and the picture of the whole are the same
/// bytes.
/// Behaviour: desktop.horizon.the-map-draws-the-world-from-its-landblocks
#[test]
fn the_maps_picture_is_the_same_on_one_thread_or_many() {
    use std::hash::{Hash, Hasher};
    let (world, _, _) = crate::session::data();
    let land = Arc::new(MapLand::of(&world.region));
    let machine = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    let drawn: Vec<(u64, Vec<u8>)> = [1, 2, machine.max(3)]
        .into_iter()
        .map(|threads| {
            let built = MapBuild::start_on(
                world.records.landblock_reader(),
                Arc::clone(&land),
                true,
                threads,
            )
            .wait()
            .expect("the map builds");
            let mut h = std::collections::hash_map::DefaultHasher::new();
            built.map.window((0, 0), built.map.size()).bgra.hash(&mut h);
            (h.finish(), built.overview.expect("the whole").bgra)
        })
        .collect();
    for (n, d) in drawn.iter().enumerate().skip(1) {
        assert_eq!(d.0, drawn[0].0, "the picture, build {n}");
        assert!(d.1 == drawn[0].1, "the picture of the whole, build {n}");
    }
}

/// The map opened in the world draws its own picture off the frame: the frame after it opens
/// starts the build and goes on without it, the picture comes while the frames go on, and the
/// game's own map picture is never read.
/// Behaviour: desktop.horizon.the-map-draws-the-world-from-its-landblocks
#[test]
fn the_map_opened_in_the_world_builds_its_picture_while_the_frames_go_on() {
    use crate::session::{doing, guarded};
    use auberean_horizon::keys::Keyed;
    use auberean_horizon::ui::map::OVERVIEW_KEY;
    guarded(5, |note| {
        doing(note, "entering the world");
        let mut s = crate::session::Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
        s.enter_the_world();
        let state = s.horizon().borrow().state().clone();
        s.horizon().borrow_mut().ui_mut().keyed(Keyed::Map, &state);
        s.step();
        assert!(
            s.horizon().borrow().ui().map.is_building(),
            "started by the frame, not drawn in it"
        );
        doing(note, "building the map");
        while s.horizon().borrow().ui().map.is_building() {
            s.step();
        }
        assert!(s.horizon().borrow().ui().map.is_ready(), "the picture came");
        s.step();
        let h = s.horizon();
        let ui = h.borrow();
        let ui = ui.ui();
        assert!(
            ui.art().lookup(OVERVIEW_KEY).flatten().is_some(),
            "the whole kept"
        );
        assert!(
            (ui.map.view.zoom - 1.0).abs() < 1e-6,
            "opened on the whole world"
        );
        let region_map = crate::session::data().0.region.region.map.0;
        assert!(
            ui.art().lookup(&format!("icon:{region_map:08x}")).is_none(),
            "the game's map picture is not read"
        );
    });
}

/// A world whose landblocks do not read gives the map no picture: the build ends, the window
/// says so and shows the sea, and asks for no other build of the same world; nothing waits on a
/// picture that will not come.
/// Behaviour: desktop.horizon.the-map-draws-the-world-from-its-landblocks
#[test]
fn a_world_with_no_land_to_read_ends_the_maps_build_without_a_picture() {
    use crate::session::{doing, guarded};
    use auberean_horizon::ui::map::MapWindow;
    guarded(5, |note| {
        doing(note, "opening a world whose landblocks do not read");
        let world = world_without_land();
        let mut map = MapWindow::default();
        map.toggle(false);
        map.keep_picture(&world);
        assert!(map.is_building());
        doing(note, "waiting for the build to end");
        while map.is_building() {
            map.keep_picture(&world);
            std::thread::yield_now();
        }
        assert!(map.is_failed() && !map.is_ready(), "no picture");
        map.keep_picture(&world);
        assert!(map.is_failed(), "not built again for the same world");
    });
}

/// The install's world, its cell file's records read through a reader that holds none of them.
fn world_without_land() -> auberean_scene::WorldData {
    use auberean_client_sdk::dat::locate::{install_dats, DatName};
    use auberean_client_sdk::dat::DatFile;
    let dir = auberean_dat::testing::require_target_install();
    let found = install_dats(&dir).expect("list the install");
    let open = |want: DatName| -> Option<DatFile> {
        let (_, p) = found.iter().find(|(n, _)| *n == want)?;
        DatFile::open(p).ok()
    };
    let portal = open(DatName::Portal).expect("a portal file");
    let cell = open(DatName::Cell(1)).expect("a cell file");
    let ctx = auberean_assets::Context::from_portal(&portal).expect("the property list");
    let records =
        auberean_world::records::WorldRecords::new(portal, cell, open(DatName::HighRes), ctx);
    records.read_cells_through(Some(Arc::new(|_, _| None)));
    auberean_scene::WorldData::new(records).expect("the region")
}

/// Write `pixels` under `name` in the shot folder, when one is named.
fn keep(name: &str, pixels: &auberean_horizon::world_map::Pixels) {
    let Some(dir) = std::env::var_os("AUBEREAN_MAP_SHOT_DIR").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the shot folder");
    let path = dir.join(format!("{name}.png"));
    let rgba: Vec<u8> = pixels
        .bgra
        .chunks(4)
        .flat_map(|p| [p[2], p[1], p[0], 255])
        .collect();
    let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), pixels.width, pixels.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}
