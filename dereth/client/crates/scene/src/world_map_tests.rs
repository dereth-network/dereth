//! Behaviour: none (this client's own world map, not a behaviour of the retail client)

use std::sync::Arc;

use super::*;

/// A world of sea over these terrains, each height-table step four metres.
fn world(names: &[&str]) -> MapGrid {
    let mut heights = [0.0; 256];
    for (i, h) in heights.iter_mut().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: a table index.
        let step = i as f32;
        *h = step * 4.0;
    }
    MapGrid::new(names.iter().map(|n| (*n).to_owned()).collect(), heights)
}

/// Land over the vertices `x`, `y`, its terrain and height-table index by vertex.
fn land(
    grid: &mut MapGrid,
    x: std::ops::Range<usize>,
    y: std::ops::Range<usize>,
    at: impl Fn(usize, usize) -> (u8, u8),
) {
    for vx in x {
        for vy in y.clone() {
            let (kind, height) = at(vx, vy);
            grid.set((vx, vy), kind, height, false);
        }
    }
}

/// Vertex `(x, y)`'s own pixel, red, green and blue.
fn pixel(map: &WorldMap, (x, y): (usize, usize)) -> [u8; 3] {
    let p = map.window((CELL_PX * x + 1, CELL_PX * (LAST - y) + 1), (1, 1));
    [p.bgra[2], p.bgra[1], p.bgra[0]]
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(&b).all(|(a, b)| a.abs_diff(*b) <= 1)
}

const NAMES: [&str; 4] = ["WaterDeepSea", "Snow", "Unheard Of", "Grassland"];
const SNOW: u8 = 1;
const UNHEARD: u8 = 2;
const GRASS: u8 = 3;

#[test]
fn each_terrain_is_drawn_in_its_name_s_colour_whatever_its_index() {
    let mut grid = world(&NAMES);
    land(&mut grid, 400..460, 400..440, |x, _| match x {
        ..420 => (SNOW, 10),
        420..440 => (UNHEARD, 10),
        _ => (GRASS, 10),
    });
    let map = grid.prepare();
    assert!(near(pixel(&map, (410, 420)), [185, 185, 185]), "Snow's");
    assert!(
        near(pixel(&map, (430, 420)), [128, 128, 128]),
        "no name: grey"
    );
    assert!(near(pixel(&map, (450, 420)), [68, 79, 39]), "Grassland's");
}

#[test]
fn a_slope_facing_the_north_west_light_is_lighter_and_one_facing_away_darker() {
    let mut grid = world(&NAMES);
    let rise = |from: usize, x: usize| u8::try_from(x.abs_diff(from)).unwrap_or(0);
    land(&mut grid, 400..440, 400..440, |x, _| (GRASS, rise(400, x)));
    land(&mut grid, 500..540, 400..440, |x, _| (GRASS, rise(540, x)));
    land(&mut grid, 600..640, 400..440, |x, _| {
        (if x < 620 { GRASS } else { SNOW }, 20)
    });
    let map = grid.prepare();
    let (east, west, flat) = (
        pixel(&map, (420, 420)),
        pixel(&map, (520, 420)),
        pixel(&map, (610, 420)),
    );
    assert!(near(flat, [68, 79, 39]), "flat: its own colour {flat:?}");
    assert!(
        east.iter().zip(&flat).all(|(a, b)| a > b),
        "rising to the east it faces the light: {east:?}"
    );
    assert!(
        west.iter().zip(&flat).all(|(a, b)| a < b),
        "rising to the west it faces away: {west:?}"
    );
}

#[test]
fn the_sea_is_lighter_along_the_shore() {
    let mut grid = world(&NAMES);
    land(&mut grid, 1000..1010, 1000..1010, |x, _| {
        (GRASS, u8::try_from(x - 1000).unwrap_or(0))
    });
    let map = grid.prepare();
    assert_eq!(pixel(&map, (1010, 1005)), SHORE, "a landcell out");
    assert_eq!(pixel(&map, (1012, 1005)), SHORE, "three out");
    assert_eq!(pixel(&map, (1013, 1005)), SEA, "four out, the open sea");
}

#[test]
fn a_grid_line_is_drawn_every_five_units_through_0e_and_0n() {
    let map = world(&NAMES).prepare();
    let at = |x: usize, y: usize| {
        let p = map.window((x, y), (1, 1));
        [p.bgra[2], p.bgra[1], p.bgra[0]]
    };
    for (x, y) in [
        (3060, 100),
        (3210, 100),
        (2910, 4000),
        (100, 3063),
        (100, 2913),
    ] {
        assert_eq!(at(x, y), GRID_COLOUR, "({x}, {y})");
    }
    assert_eq!(at(3061, 100), SEA);
    assert_eq!(at(100, 3064), SEA);
}

#[test]
fn placeholder_land_is_left_out_and_real_land_kept() {
    let mut grid = world(&NAMES);
    let hilly = |x: usize, y: usize| (GRASS, u8::try_from((x + y) % 7).unwrap_or(0));
    // At the world's west edge.
    land(&mut grid, 0..20, 500..520, hilly);
    // Where the far north's first placeholder desert stands, 77.5E 78.5N.
    land(&mut grid, 1795..1820, 1805..1830, hilly);
    // All one terrain, dead flat.
    land(&mut grid, 800..830, 800..830, |_, _| (SNOW, 30));
    // Kept: flat but of two terrains, one terrain over hills, and a flat islet.
    land(&mut grid, 900..930, 800..830, |x, _| {
        (if x < 915 { SNOW } else { GRASS }, 30)
    });
    land(&mut grid, 1000..1030, 800..830, hilly);
    land(&mut grid, 1100..1108, 800..810, |_, _| (SNOW, 30));
    let dropped = grid.clone().drop_placeholder_land();
    let why: Vec<Why> = dropped.iter().map(|d| d.why).collect();
    assert_eq!(why, [Why::Edge, Why::Flat, Why::Desert], "{dropped:?}");
    assert_eq!(dropped[1].vertices, 900);
    assert!(
        (dropped[2].south_west.0 - 77.55).abs() < 1e-6,
        "{dropped:?}"
    );
    let map = grid.prepare();
    for gone in [(10, 510), (1807, 1817), (815, 815)] {
        assert_eq!(pixel(&map, gone), SEA, "{gone:?}");
    }
    assert!(near(pixel(&map, (905, 815)), [185, 185, 185]));
    assert_ne!(pixel(&map, (1015, 815)), SEA);
    assert!(near(pixel(&map, (1104, 805)), [185, 185, 185]));
}

/// A map with something on it, as the tests below compare its pictures.
fn island() -> WorldMap {
    let mut grid = world(&NAMES);
    land(&mut grid, 300..700, 1500..1900, |x, y| {
        (
            if (x / 13 + y / 17) % 2 == 0 {
                SNOW
            } else {
                GRASS
            },
            u8::try_from((x * 7 + y * 3) % 61).unwrap_or(0),
        )
    });
    for x in 350..650 {
        grid.set((x, 1700), GRASS, 5, true);
    }
    grid.prepare()
}

#[test]
fn the_tiles_are_the_picture_cut_up_and_the_last_are_short() {
    let map = island();
    let tile = map.tile((0, 1));
    assert_eq!((tile.width, tile.height), (1024, 1024));
    let across = map.window((512, 1024), (1024, 4));
    for r in 0..4 {
        let row = &tile.bgra[(r * 1024 + 512) * 4..(r * 1024 + 1024) * 4];
        assert_eq!(row, &across.bgra[r * 4096..r * 4096 + 2048], "row {r}");
    }
    let corner = map.tile((TILES - 1, TILES - 1));
    assert_eq!((corner.width, corner.height), (1003, 1003));
}

#[test]
fn the_overview_is_the_picture_shrunk_and_drawn_alike_in_bands_or_whole() {
    let map = island();
    let overview = map.overview();
    assert_eq!((overview.width, overview.height), (2048, 2048));
    let spans = spans(SIDE, OVERVIEW);
    // Each overview pixel against the picture's pixels it covers.
    for (ox, oy) in [
        (200, 200),
        (300, 140),
        (420, 300),
        (433, 341),
        (500, 400),
        (1500, 1500),
    ] {
        let (xs, ys) = (&spans[ox], &spans[oy]);
        let picture = map.window((xs[0].0, ys[0].0), (xs.len(), ys.len()));
        let mut want = [0.0f64; 3];
        for (r, &(_, cy)) in ys.iter().enumerate() {
            for (c, &(_, cx)) in xs.iter().enumerate() {
                for (ch, w) in want.iter_mut().enumerate() {
                    let at = (r * xs.len() + c) * 4 + 2 - ch;
                    *w += f64::from(picture.bgra[at]) * cx * cy;
                }
            }
        }
        let at = (oy * OVERVIEW + ox) * 4;
        let got = [
            overview.bgra[at + 2],
            overview.bgra[at + 1],
            overview.bgra[at],
        ];
        for (g, w) in got.iter().zip(want) {
            assert!(
                (f64::from(*g) - w).abs() <= 0.5 + 1e-3,
                "({ox}, {oy}): {got:?} {want:?}"
            );
        }
    }
    // A band drawn alone, as a build in the frame's own thread draws it.
    let mut band = vec![0; 16 * OVERVIEW * 4];
    map.overview_rows(&spans, 300, &mut band);
    assert_eq!(band, overview.bgra[300 * OVERVIEW * 4..316 * OVERVIEW * 4]);
}

/// FNV-1a over the pixels' red, green and blue.
fn rgb_hash(p: &Pixels) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for px in p.bgra.as_chunks::<4>().0 {
        for b in [px[2], px[1], px[0]] {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The later data draws the pictures the map was first drawn with offline, to the byte.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats: --features retail-dats"
)]
fn the_retail_data_draws_the_map_s_first_pictures() {
    let store = Arc::new(dereth_dat::testing::open_store().expect("the retail dats"));
    let built = MapBuild::start(store, true).wait().expect("the map builds");
    assert_eq!(rgb_hash(&built.map.tile((3, 2))), 0x253a_3e35_21ac_301c);
    let overview = built.overview.expect("the overview");
    assert_eq!(rgb_hash(&overview), 0x10b8_2333_5c20_6c04);
}

/// How long the map takes to build from each data set there is, and what it leaves out; with
/// `DERETH_WORLD_MAP_OUT` set, each overview is written there as a PNG.
#[test]
#[ignore = "an instrument: reads $DERETH_TEST_DAT_DIR and $DERETH_TEST_PRETOD_DAT_DIR"]
fn instrument_world_map_timings() {
    use web_time::Instant;
    let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
    let mut sets = Vec::new();
    if let Some(store) = dereth_dat::testing::open_store() {
        sets.push(("modern", store));
    }
    if dereth_dat::testing::classic_shortfall().is_none() {
        sets.push(("classic", dereth_dat::testing::open_classic_store_or_fail()));
    }
    assert!(!sets.is_empty(), "no data set to time");
    for (label, store) in sets {
        let store = Arc::new(store);
        let t = Instant::now();
        let built = MapBuild::start(Arc::clone(&store), true)
            .wait()
            .expect("the map builds");
        let first = ms(t);
        let t = Instant::now();
        let mut grid = MapGrid::read(&store).expect("read");
        let read = ms(t);
        let t = Instant::now();
        let dropped = grid.drop_placeholder_land();
        let placeholders = ms(t);
        let water = grid.water_kinds();
        let t = Instant::now();
        let distance = grid.shore_distance(&water);
        let shore = ms(t);
        let t = Instant::now();
        let shade = grid.shading();
        let shading = ms(t);
        let blocks = grid.blocks;
        let map = WorldMap::new(grid, water, distance, shade);
        let t = Instant::now();
        let overview = map.overview();
        let whole = ms(t);
        assert_eq!(overview, built.overview.clone().expect("overview"));
        let mut tiles = Vec::new();
        for (col, row) in [(3, 2), (4, 1), (5, 5)] {
            let t = Instant::now();
            std::hint::black_box(map.tile((col, row)));
            tiles.push(format!("{col}-{row} {:.1}", ms(t)));
        }
        let t = Instant::now();
        let serial: Vec<u8> = (0..TILE)
            .flat_map(|r| {
                let mut row = vec![0; TILE * 4];
                map.draw_row(2 * TILE + r, 3 * TILE, &mut row);
                row
            })
            .collect();
        let tile_serial = ms(t);
        assert_eq!(serial, map.tile((3, 2)).bgra);
        eprintln!(
            "{label}: {} landblocks; first overview {first:.0} ms (read {read:.0}, placeholders \
             {placeholders:.0}, shore {shore:.0}, shading {shading:.0}, overview {whole:.0}); \
             tiles ms {tiles:?}, 3-2 on one thread {tile_serial:.1}; tile 3-2 {:#x}",
            blocks,
            rgb_hash(&map.tile((3, 2)))
        );
        for d in &dropped {
            eprintln!("  left out {d:?}");
        }
        if let Some(dir) = std::env::var_os("DERETH_WORLD_MAP_OUT") {
            let path = std::path::Path::new(&dir).join(format!("world-map-{label}.png"));
            let file = std::fs::File::create(&path).expect("the PNG");
            let mut png = png::Encoder::new(std::io::BufWriter::new(file), 2048, 2048);
            png.set_color(png::ColorType::Rgb);
            let rgb: Vec<u8> = overview
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0]])
                .collect();
            png.write_header()
                .and_then(|mut w| w.write_image_data(&rgb))
                .expect("written");
            eprintln!("  wrote {}", path.display());
        }
    }
}
