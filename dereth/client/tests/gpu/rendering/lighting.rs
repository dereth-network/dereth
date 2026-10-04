//! Scene lighting: resident light pools, interior vertex lighting and outdoor sunlight. Lights
//! come from the setup entries of resident statics; the static pool is rebuilt from the resident
//! cells and sorted by squared distance from the player, and a white viewer light (falloff 10,
//! intensity 2.25) rides 2 m above the player. An object draw selects at most eight lights,
//! dynamics first in distance order and then statics; a cell mesh has its static lights burned
//! into its vertices, and an outdoor draw is lit by the sun. Unseen interiors take a white 0.2
//! ambient. Fixture: the retail dats' training academy (0x8602) and outdoor Holtburg drawn on a
//! software device with a local body; no datagrams. The burn and the selection are checked against
//! standalone evaluations kept in this file; image checks compare runs with lighting on and off,
//! or with a frame-matched untreated control.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client::world::{SceneReads, SceneWrites};
use std::sync::Arc;

use dereth_animation::parts::LightingMode;
use dereth_assets::Decode;
use dereth_client::character::CharacterInput;
use dereth_client::env_cells::{cell_statics, EnvCellLoader};
use dereth_client::objects::ObjectStream;
use dereth_client::world::{SceneConfig, WorldScene, DEFAULT_LANDBLOCK};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{CellId, DataId, LocalTime, Vec3};
use dereth_render::device::Gpu;
use dereth_world_render::lighting::{
    minimize_object_lighting, LightInfo, LightPools, CALC_POINT_LIGHT_FALLOFF_MULTIPLIER,
    HARDWARE_LIGHT_SLOTS, INDOOR_AMBIENT_LEVEL,
};

const W: u32 = 400;
const H: u32 = 300;

/// The training academy -- Holtburg's dungeon.
const ACADEMY: u16 = 0x8602;

/// The academy cell a new character enters (starter-area entry 0). It stands in for the reported
/// starter room, whose screenshots carry no cell or position, so it is a candidate station rather
/// than an exact camera registration.
const REPORTED_STATION_CANDIDATE: CellId = CellId(0x8602_01AD);

/// Six fixed updates before returning the station image; this is not a measured settling time.
const FRAMES: u32 = 6;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

/// Raw setup-local light records from a decoded setup. Non-setup IDs and failed reads/decodes
/// return an empty list. This helper does not apply the owning static's frame to light offsets.
fn setup_lights(store: &RetailDatStore, id: DataId) -> Vec<dereth_assets::geometry::LightInfo> {
    if dereth_dat::divine_type(id) != Some(DbType::Setup) {
        return Vec::new();
    }
    let Ok(bytes) = store.read_typed(DbType::Setup, id) else {
        return Vec::new();
    };
    let Ok(setup) = dereth_assets::Setup::decode_payload(id, &bytes) else {
        return Vec::new();
    };
    setup.lights.values().copied().collect()
}

/// One row per academy static whose decoded setup has light entries: its cell, setup ID,
/// static-frame origin and entry count. The origin is not each transformed light position.
fn torch_cells(store: &RetailDatStore) -> Vec<(CellId, DataId, Vec3, usize)> {
    let mut out = Vec::new();
    for d in EnvCellLoader::new().load_block(store, ACADEMY) {
        for s in cell_statics(&d) {
            let lights = setup_lights(store, s.id);
            if !lights.is_empty() {
                out.push((d.id, s.id, s.frame.origin, lights.len()));
            }
        }
    }
    out
}

/// Load the configured indoor/outdoor scene and character, then run the helper's local
/// sync/update/stream/draw order for six frames and return its last capture. No session input
/// is replayed through the empty object stream.
fn station(store: &Arc<RetailDatStore>, gpu: &mut Gpu, cfg: SceneConfig) -> (WorldScene, Vec<u8>) {
    let region = dereth_client::world::load_region(store).expect("the region decodes");
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    let mut stream = ObjectStream::new();
    let rgba = frames(&mut scene, gpu, store, &mut stream, 0, FRAMES);
    (scene, rgba)
}

/// `count` more frames of a station, from frame `from`; the last one captured.
fn frames(
    scene: &mut WorldScene,
    gpu: &mut Gpu,
    store: &Arc<RetailDatStore>,
    stream: &mut ObjectStream,
    from: u32,
    count: u32,
) -> Vec<u8> {
    let mut rgba = Vec::new();
    for i in from..from + count {
        let t = f64::from(i + 1) * dereth_client::app::HEADLESS_STEP;
        scene
            .sync_objects(store, gpu, stream)
            .expect("sync_objects");
        scene.update(
            dereth_client::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(t),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        rgba = gpu.capture().expect("capture").to_rgba();
    }
    rgba
}

fn luma(p: &[u8]) -> f32 {
    (f32::from(p[0]) + f32::from(p[1]) + f32::from(p[2])) / 3.0
}

/// The mean luma over a mask of pixel indices.
fn mean_over(rgba: &[u8], mask: &[usize]) -> f64 {
    if mask.is_empty() {
        return 0.0;
    }
    let sum: f64 = mask
        .iter()
        .map(|&i| f64::from(luma(&rgba[i * 4..i * 4 + 4])))
        .sum();
    sum / mask.len() as f64
}

/// Pixels whose absolute difference in mean RGB exceeds 2. This is a differential mask, not
/// segmented object/cell coverage; opposing RGB changes can cancel in the mean.
fn lit_mask(lit: &[u8], unlit: &[u8]) -> Vec<usize> {
    (0..lit.len() / 4)
        .filter(|&i| (luma(&lit[i * 4..i * 4 + 4]) - luma(&unlit[i * 4..i * 4 + 4])).abs() > 2.0)
        .collect()
}

fn indoor_cfg(cell: CellId, lighting: bool) -> SceneConfig {
    SceneConfig {
        landblock: ACADEMY,
        start_cell: Some(cell),
        time_of_day: Some(0.5),
        object_lighting: lighting,
        ..SceneConfig::default()
    }
}

fn outdoor_cfg(time_of_day: f32, lighting: bool) -> SceneConfig {
    SceneConfig {
        landblock: DEFAULT_LANDBLOCK,
        time_of_day: Some(time_of_day),
        object_lighting: lighting,
        ..SceneConfig::default()
    }
}

/// The candidate starter room has the whole indoor lighting chain: registered resident lights and
/// a static pool, the unseen-interior ambient, burned vertices across the drawn cells and an
/// active detail stage. Lighting differences count pixels; detail differences count bytes.
/// Neither is a comparison with a retail capture.
///
/// The static pool is populated from the resident cells, not filtered to the cells drawn; this
/// station does not prove the resident block enumerates exactly the cells retail holds.
#[test]
fn the_holtburg_starter_cell_has_the_complete_indoor_lighting_chain() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);

    gpu.clear_stage1_binds();
    let (scene, lit) = station(
        &store,
        &mut gpu,
        indoor_cfg(REPORTED_STATION_CANDIDATE, true),
    );
    let detail_binds = gpu.stage1_binds();

    let pools = scene.light_pools();
    assert!(
        scene.cell_light_objects() > 0,
        "the candidate room registered no resident light objects"
    );
    assert!(
        !pools.statics.is_empty(),
        "the candidate room's static pool is empty"
    );
    assert!(
        pools
            .statics
            .iter()
            .all(|l| !dereth_physics::landdefs::is_outdoors(l.cell_id)),
        "an outdoor cell entered the resident interior static pool: {:?}",
        pools.statics.iter().map(|l| l.cell_id).collect::<Vec<_>>()
    );
    let ambient = scene.world_ambient_color();
    assert_eq!(
        ambient, [INDOOR_AMBIENT_LEVEL; 3],
        "the candidate room took the outdoor/seen-outside ambient arm"
    );

    let drawn = scene
        .drawn_cells()
        .expect("the candidate station completed an indoor draw");
    assert!(
        drawn.contains(&REPORTED_STATION_CANDIDATE.0),
        "the body cell was not drawn: {drawn:?}"
    );
    let mut burned = 0usize;
    let mut coloured = 0usize;
    for cell in drawn.iter().copied().map(CellId) {
        for (_, _, rgb) in scene.env_cell_burned_vertices(cell) {
            burned += 1;
            coloured += usize::from(rgb != [0; 3]);
        }
    }
    // This totals burned vertices over all drawn cells; the aggregate is pinned, which does not
    // prove the candidate cell's own topology or the full traversal.
    assert_eq!(
        burned, 84,
        "the candidate station total over drawn-cell burned vertices changed"
    );
    assert!(
        coloured > 10,
        "only {coloured}/{burned} drawn vertices received a static-light burn"
    );
    assert!(
        detail_binds > 0,
        "the environment detail surface was generated but never bound"
    );

    gpu.clear_stage1_binds();
    let mut no_detail_cfg = indoor_cfg(REPORTED_STATION_CANDIDATE, true);
    no_detail_cfg.render.environment_detail_textures = false;
    let (_, no_detail) = station(&store, &mut gpu, no_detail_cfg);
    assert_eq!(
        gpu.stage1_binds(),
        0,
        "the detail-off control still bound stage 1"
    );

    let (_, unlit) = station(
        &store,
        &mut gpu,
        indoor_cfg(REPORTED_STATION_CANDIDATE, false),
    );
    let lighting_pixels = lit
        .chunks_exact(4)
        .zip(unlit.chunks_exact(4))
        .filter(|(a, b)| (luma(a) - luma(b)).abs() > 2.0)
        .count();
    let detail_pixels = lit.iter().zip(&no_detail).filter(|(a, b)| a != b).count();
    eprintln!(
        "starter room {:#010X}: resident light objects {}, pool {}/{}, ambient {ambient:?}, \
         burned {coloured}/{burned}, detail binds {detail_binds}, lighting pixels {lighting_pixels}, \
         detail bytes {detail_pixels}",
        REPORTED_STATION_CANDIDATE.0,
        scene.cell_light_objects(),
        pools.statics.len(),
        pools.max_static,
    );
    assert!(
        lighting_pixels > 1_000,
        "lighting moves only {lighting_pixels} candidate-station pixels"
    );
    assert!(
        detail_pixels > 1_000,
        "detail moves only {detail_pixels} candidate-station bytes"
    );
}

// -------------------------------------------------------------------------------------------
// 0. The instrument can look: the academy's torches reach the pool.
// -------------------------------------------------------------------------------------------

/// Behaviour: rendering.lighting.resident-light-pools-and-sunlight-light-the-scene
/// Decode the academy's light-bearing statics and stand in the first listed cell. Require
/// registered light objects, a nonempty distance-sorted static pool, the viewer-light defaults
/// and white indoor ambient; with no registration, `cell_light_objects()` would be 0. The metadata
/// enumeration does not assert that each setup light appears in this pool.
#[test]
fn a_torch_in_an_academy_cell_enters_the_static_pool_when_the_body_stands_there() {
    let store = store();
    let torches = torch_cells(&store);
    assert!(
        !torches.is_empty(),
        "no academy cell contains a static whose setup record carries lights"
    );
    for (cell, id, at, n) in torches.iter().take(12) {
        eprintln!(
            "lighting: cell {:#010X} static {:#010X} lights {n} at ({:.2}, {:.2}, {:.2})",
            cell.0, id.0, at.x, at.y, at.z
        );
    }
    let mut gpu = crate::common::test_gpu(W, H);
    let (cell, _, _, _) = torches[0];
    let (scene, _) = station(&store, &mut gpu, indoor_cfg(cell, true));
    assert!(
        scene.cell_light_objects() > 0,
        "no light object was registered for the academy statics"
    );
    let pools = scene.light_pools();
    assert!(
        !pools.statics.is_empty(),
        "the static pool is empty with {} light objects registered",
        scene.cell_light_objects()
    );
    // Check the stored squared-distance ordering of the static pool.
    for w in pools.statics.windows(2) {
        assert!(
            w[0].distance_sq <= w[1].distance_sq,
            "the static pool is not sorted"
        );
    }
    // Inspect the dynamic pool's first entry, the viewer light: white, falloff 10, and intensity
    // 2.25 (0.5 * 4.5), which startup sets before the first frame. This is not a claim about every
    // frame's pool ordering.
    assert!(
        !pools.dynamics.is_empty(),
        "the dynamic pool has no viewer-light entry"
    );
    assert_eq!(pools.dynamics[0].info.intensity, 2.25);
    assert_eq!(pools.dynamics[0].info.falloff, 10.0);
    assert_eq!(
        pools.dynamics[0].info.color, [1.0; 3],
        "the viewer light starts white, 0xFFFFFFFF"
    );
    // Unseen-interior ambient: level 0.2 with white color 0xFFFFFFFF.
    let amb = scene.world_ambient_color();
    assert!(
        (amb[0] - INDOOR_AMBIENT_LEVEL).abs() < 1e-6 && amb == [amb[0]; 3],
        "the academy ambient is not the 0.2 white of the unseen-interior branch: {amb:?}"
    );
}

// -------------------------------------------------------------------------------------------
// 1. A torch-lit wall falls off with distance, with calc_point_light's own shape.
// -------------------------------------------------------------------------------------------

/// Standalone evaluation of the retail point-light vertex burn: range = falloff * 1.3, a wrapped
/// dot `(N.d + distance/2) * 2/3`, denominator `distance_squared > 1 ? distance_squared * distance :
/// distance`, and each channel capped at the light's colour before the sum is clamped to [0, 1].
/// It does not call the production helper, but receives positions and lights from the scene.
fn retail_calc_point_light(
    v: Vec3,
    n: Vec3,
    light: Vec3,
    colour: [f32; 3],
    intensity: f32,
    falloff: f32,
    acc: &mut [f32; 3],
) {
    let d = Vec3::new(light.x - v.x, light.y - v.y, light.z - v.z);
    let d2 = d.x * d.x + d.y * d.y + d.z * d.z;
    let dist = d2.sqrt();
    let range = falloff * 1.3;
    if dist >= range {
        return;
    }
    let n_dot = (n.x * d.x + n.y * d.y + n.z * d.z + dist * 0.5) * 0.666_666_7;
    if n_dot <= 0.0 {
        return;
    }
    let denom = if d2 > 1.0 { d2 * dist } else { dist };
    let s = (1.0 - dist / range) * intensity * (n_dot / denom);
    for c in 0..3 {
        acc[c] += (s * colour[c]).min(colour[c]);
    }
}

/// Clamp the accumulated channel to [0,1], multiply by 255 and convert with dereth_primitives::num::to_i32
/// before clamping to a byte. This conversion helper is shared with production code.
fn burn_byte(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    dereth_primitives::num::to_i32(c * 255.0).clamp(0, 255) as u8
}

/// Compare burned channels for the listed torch cells that have a scene block origin. Evaluate
/// every static light from the production pool in each cell's translated coordinates, then
/// require more than 300 channels with no discrepancy greater than one byte. This is not an
/// audit of every resident cell or an assertion of exact byte equality.
///
/// Bin maximum burned channels by distance to the nearest static light: four 2 m intervals and
/// a final >= 8 m bucket. Every bucket needs ten samples; successive means may rise by up to 4,
/// the first must exceed the last by 60 and exceed 120. No predicate requires a black far bucket.
///
/// A separate lit/unlit frame comparison requires more than 2,000 changed pixels and a mean-RGB
/// ratio above ambient+0.05 and below 0.95 over that mask. The falloff is read from the burned
/// vertices rather than from screen-space rings, because Gouraud interpolation across a coarse
/// wall's distant corners flattens the rings.
#[test]
fn a_torch_lit_wall_falls_off_with_distance_as_calc_point_light_says() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let torches = torch_cells(&store);
    assert!(
        !torches.is_empty(),
        "no academy cell contains a static whose setup record carries lights"
    );
    let cell = torches[0].0;
    let (scene, lit) = station(&store, &mut gpu, indoor_cfg(cell, true));
    let pools = scene.light_pools();
    assert!(
        !pools.statics.is_empty(),
        "{:#010X}: the static pool is empty",
        cell.0
    );

    // Compare available burned vertices in listed torch cells with the standalone formula.
    let mut cells: Vec<CellId> = torches.iter().map(|t| t.0).collect();
    cells.dedup();
    let mut mismatches = 0usize;
    let mut checked = 0usize;
    // (distance to the nearest static light, max channel of the burn)
    let mut samples: Vec<(f32, u8)> = Vec::new();
    for c in &cells {
        let Some(origin) = scene.cell_block_origin(*c) else {
            continue;
        };
        let lights: Vec<(Vec3, [f32; 3], f32, f32)> = pools
            .statics
            .iter()
            .map(|l| {
                let o = l.info.offset.origin;
                (
                    Vec3::new(o.x - origin.0, o.y - origin.1, o.z),
                    l.info.color,
                    l.info.intensity,
                    l.info.falloff,
                )
            })
            .collect();
        for (pos, normal, rgb) in scene.env_cell_burned_vertices(*c) {
            let mut acc = [0.0f32; 3];
            let mut nearest = f32::MAX;
            for (lp, colour, intensity, falloff) in &lights {
                retail_calc_point_light(pos, normal, *lp, *colour, *intensity, *falloff, &mut acc);
                let d = ((lp.x - pos.x).powi(2) + (lp.y - pos.y).powi(2) + (lp.z - pos.z).powi(2))
                    .sqrt();
                nearest = nearest.min(d);
            }
            let want = [burn_byte(acc[0]), burn_byte(acc[1]), burn_byte(acc[2])];
            for k in 0..3 {
                checked += 1;
                if (i32::from(want[k]) - i32::from(rgb[k])).abs() > 1 {
                    mismatches += 1;
                }
            }
            samples.push((nearest, rgb[0].max(rgb[1]).max(rgb[2])));
        }
    }
    assert!(checked > 300, "only {checked} burned channels to check");
    assert_eq!(mismatches, 0, "{mismatches} of {checked} burned channels differ by more than one byte from the standalone point-light evaluation");

    // Nearest-light distance buckets: 0..2,2..4,4..6,6..8 and >= 8 m. The last is open-ended.
    // Allow a four-byte rise between adjacent means; require a strong near/far difference.
    const BIN_M: f32 = 2.0;
    const BINS: usize = 5;
    let mut sums = [0.0f64; BINS];
    let mut counts = [0usize; BINS];
    for (d, v) in &samples {
        let b = ((d / BIN_M) as usize).min(BINS - 1);
        sums[b] += f64::from(*v);
        counts[b] += 1;
    }
    let means: Vec<f64> = (0..BINS)
        .map(|b| sums[b] / counts[b].max(1) as f64)
        .collect();
    eprintln!("lighting: {} vertices over {} cells, {} static lights in the pool; burn by 2 m bin from the nearest torch: {means:?} over {counts:?}", samples.len(), cells.len(), pools.statics.len());
    assert!(
        counts.iter().all(|&c| c >= 10),
        "a distance bin is too thin: {counts:?}"
    );
    for w in means.windows(2) {
        assert!(
            w[1] <= w[0] + 4.0,
            "the burn rises with distance from the torch: {means:?}"
        );
    }
    assert!(
        means[0] > means[BINS - 1] + 60.0,
        "no measurable falloff from the torch: {means:?}"
    );
    assert!(
        means[0] > 120.0,
        "the vertices next to a torch are not bright: {means:?}"
    );

    // Compare mean RGB over pixels changed by the lighting switch, not an object/cell mask.
    let (_, unlit) = station(&store, &mut gpu, indoor_cfg(cell, false));
    assert_ne!(
        lit, unlit,
        "{:#010X}: the lighting switch changed no pixel",
        cell.0
    );
    let mask = lit_mask(&lit, &unlit);
    let (m_lit, m_unlit) = (mean_over(&lit, &mask), mean_over(&unlit, &mask));
    let ratio = m_lit / m_unlit;
    eprintln!("lighting: {:#010X}: {} pixels changed by the switch; lit {m_lit:.2} unlit {m_unlit:.2} ratio {ratio:.3}", cell.0, mask.len());
    assert!(
        mask.len() > 2000,
        "{:#010X}: the switch changed only {} pixels",
        cell.0,
        mask.len()
    );
    assert!(ratio > INDOOR_AMBIENT_LEVEL as f64 + 0.05, "{:#010X}: the changed-pixel mean ratio ({ratio:.3}) did not exceed the ambient-level-plus-0.05 threshold", cell.0);
    assert!(
        ratio < 0.95,
        "{:#010X}: the lit cell reads as bright as unlit ({ratio:.3})",
        cell.0
    );
}

// -------------------------------------------------------------------------------------------
// 2. Outdoors: day and night under the sunlight set.
// -------------------------------------------------------------------------------------------

/// Separate noon/midnight scenes with lighting on/off. Require stronger noon sun diffuse,
/// then reuse the noon difference mask for all four means. Noon lit must be below noon unlit;
/// midnight lit must be more than 5 below noon lit. Midnight unlit is reported but has no
/// corresponding darker-than-unlit assertion. These image tests do not count active sun slots.
#[test]
fn an_outdoor_station_is_lit_by_the_sun_and_darker_at_night() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    let (day, day_lit) = station(&store, &mut gpu, outdoor_cfg(0.5, true));
    let (_, day_unlit) = station(&store, &mut gpu, outdoor_cfg(0.5, false));
    let (night, night_lit) = station(&store, &mut gpu, outdoor_cfg(0.0, true));
    let (_, night_unlit) = station(&store, &mut gpu, outdoor_cfg(0.0, false));

    let sun_day = day.sun_light().expect("noon has a valid sun");
    let sun_night = night.sun_light().expect("midnight has a valid sun");
    let mag = |d: [f32; 3]| (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    eprintln!(
        "lighting: sun diffuse noon {:?} midnight {:?}; ambient noon {:?} midnight {:?}",
        sun_day.diffuse,
        sun_night.diffuse,
        day.world_ambient_color(),
        night.world_ambient_color()
    );
    assert!(
        mag(sun_day.diffuse) > mag(sun_night.diffuse),
        "the sun is not brighter at noon"
    );

    let mask = lit_mask(&day_lit, &day_unlit);
    assert!(
        mask.len() > 500,
        "the lighting switch changed only {} pixels at noon",
        mask.len()
    );
    let (d_lit, d_unlit) = (mean_over(&day_lit, &mask), mean_over(&day_unlit, &mask));
    let (n_lit, n_unlit) = (mean_over(&night_lit, &mask), mean_over(&night_unlit, &mask));
    eprintln!("lighting: over {} noon difference-mask pixels -- noon lit {d_lit:.2} unlit {d_unlit:.2}; midnight lit {n_lit:.2} unlit {n_unlit:.2}", mask.len());
    assert!(d_lit < d_unlit, "noon: the difference-mask lit mean is not below the unlit mean ({d_lit:.2} vs {d_unlit:.2})");
    assert!(n_lit < d_lit - 5.0, "the difference-mask midnight mean is not more than five below noon ({n_lit:.2} vs {d_lit:.2})");
}

// -------------------------------------------------------------------------------------------
// 3. Eight-slot cap and order against a separate implementation of the observed selection.
// -------------------------------------------------------------------------------------------

/// Standalone ordered selection: reaching dynamics first, then statics, with a shared cap of
/// eight. Non-point lights always reach; point lights use the squared falloff-plus-radius test.
/// Returned class/index pairs correspond to slot order for a newly reset active-light set.
fn retail_minimize(pools: &LightPools, centre: Vec3, radius: f32) -> Vec<(i32, i32)> {
    let reach = |l: &LightInfo| -> bool {
        if l.light_type != dereth_world_render::lighting::LightType::Point {
            return true;
        }
        let v = l.viewerspace_location;
        let d = [v.x - centre.x, v.y - centre.y, v.z - centre.z];
        let r = l.falloff + radius;
        (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) - r * r < 0.0002
    };
    let mut used = 0;
    let mut out = Vec::new();
    for (i, l) in pools.dynamics.iter().enumerate() {
        if used < 8 && reach(&l.info) {
            out.push((2, i as i32));
            used += 1;
        }
    }
    for (i, l) in pools.statics.iter().enumerate() {
        if used < 8 && reach(&l.info) {
            out.push((1, i as i32));
            used += 1;
        }
    }
    out
}

/// Populate six dynamic and twelve static candidates in unsorted insertion order. At the
/// origin/radius 1, all six dynamics reach and fill slots before the two nearest reaching statics.
/// More than two statics lie outside that sphere. Compare exact ordered class/index lists with
/// the standalone helper, then probe 100 m (none) and 7 m/radius 0.5 (a different capped mixture).
#[test]
fn the_per_draw_selection_caps_at_eight_dynamics_first_in_distance_order() {
    let mut pools = LightPools::new(0.0);
    assert_eq!(
        (pools.max_static, pools.max_dynamic),
        (40, 7),
        "deg_mul 0 caps"
    );
    let at = |x: f32| {
        let mut info = LightInfo {
            falloff: 5.0,
            ..LightInfo::default()
        };
        info.offset.origin = Vec3::new(x, 0.0, 0.0);
        info.viewerspace_location = Vec3::new(x, 0.0, 0.0);
        info
    };
    for x in [3.0f32, 1.0, 2.0, 6.0, 4.0, 5.0] {
        pools.add_dynamic(at(x), CellId(0), x * x);
    }
    for x in [
        2.5f32, 9.0, 1.5, 3.5, 40.0, 0.5, 4.5, 5.5, 6.5, 7.0, 7.5, 8.0,
    ] {
        pools.add_static(at(x), CellId(0), x * x);
    }
    assert_eq!(pools.dynamics.len(), 6);
    assert_eq!(pools.statics.len(), 12);
    // Sorted ascending by distance, both pools.
    let sx: Vec<f32> = pools
        .statics
        .iter()
        .map(|l| l.info.offset.origin.x)
        .collect();
    assert_eq!(
        sx,
        vec![0.5, 1.5, 2.5, 3.5, 4.5, 5.5, 6.5, 7.0, 7.5, 8.0, 9.0, 40.0]
    );

    let (centre, radius) = (Vec3::ZERO, 1.0);
    let want = retail_minimize(&pools, centre, radius);
    assert_eq!(
        want.len(),
        8,
        "the standalone loop stops at eight: {want:?}"
    );
    assert_eq!(
        &want[..6],
        &[(2, 0), (2, 1), (2, 2), (2, 3), (2, 4), (2, 5)]
    );
    assert_eq!(
        &want[6..],
        &[(1, 0), (1, 1)],
        "then the two nearest reaching statics"
    );

    let active = minimize_object_lighting(&pools, centre, radius);
    let got: Vec<(i32, i32)> = active
        .cur
        .iter()
        .filter(|s| s.light_class != -1)
        .map(|s| (s.light_class, s.index))
        .collect();
    assert_eq!(
        got, want,
        "production light selection disagrees with the standalone ordered selection"
    );
    assert_eq!(active.active(), HARDWARE_LIGHT_SLOTS);

    // At 100 m nothing reaches. At 7 m some dynamics still reach, but the 1 m dynamic and 40 m static
    // do not; compare the complete mixed selection and its eight-slot cap.
    let far = minimize_object_lighting(&pools, Vec3::new(100.0, 0.0, 0.0), 1.0);
    assert_eq!(far.active(), 0, "nothing reaches an object 100 m away");
    let near = minimize_object_lighting(&pools, Vec3::new(7.0, 0.0, 0.0), 0.5);
    let near_got: Vec<(i32, i32)> = near
        .cur
        .iter()
        .filter(|s| s.light_class != -1)
        .map(|s| (s.light_class, s.index))
        .collect();
    assert_eq!(
        near_got,
        retail_minimize(&pools, Vec3::new(7.0, 0.0, 0.0), 0.5)
    );
    assert_eq!(near_got.len(), 8, "the cap bites here too: {near_got:?}");
    assert!(
        !near_got.contains(&(1, 11)),
        "the 40 m static (index 11) must never be selected: {near_got:?}"
    );
    assert!(
        !near_got.contains(&(2, 0)),
        "the 1 m dynamic does not reach an object at 7 m: {near_got:?}"
    );
    let _ = CALC_POINT_LIGHT_FALLOFF_MULTIPLIER;
}

// -------------------------------------------------------------------------------------------
// 4. The selection blink: the bright half exceeds the baseline.
// -------------------------------------------------------------------------------------------

/// With objects lit, the blink's bright phase is brighter than the baseline (unlit, the two read
/// equal). Directly apply High (0.99,1.0), Low (0.0,0.35) and Restore to the current body, comparing each result with the same frame index of a separate untreated scene.
/// This tests material application, not the selection-blink timer or message-dispatch route.
///
/// Count whole-frame pixels whose mean RGB differs by more than 8 in either direction. High
/// and Low require at least 100 changes in the intended direction and at most 1/20 as many in
/// the opposite direction. Restore allows up to 8 above-threshold pixels; it is not exact image
/// restoration or a body-only mask. Frame-matched controls keep idle-animation changes from being
/// counted as lighting changes.
#[test]
fn the_selection_blink_bright_half_now_exceeds_the_baseline() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);
    // The control: frames 6, 7 and 8 of an untouched station.
    let (mut control, _) = station(&store, &mut gpu, outdoor_cfg(0.5, true));
    let mut cs = ObjectStream::new();
    let c6 = frames(&mut control, &mut gpu, &store, &mut cs, FRAMES, 1);
    let c7 = frames(&mut control, &mut gpu, &store, &mut cs, FRAMES + 1, 1);
    let c8 = frames(&mut control, &mut gpu, &store, &mut cs, FRAMES + 2, 1);

    let (mut scene, _) = station(&store, &mut gpu, outdoor_cfg(0.5, true));
    let body = scene.character.as_ref().expect("a body").object_id();
    let mut stream = ObjectStream::new();
    assert!(scene.apply_object_lighting(body, LightingMode::High));
    let bright = frames(&mut scene, &mut gpu, &store, &mut stream, FRAMES, 1);
    assert!(scene.apply_object_lighting(body, LightingMode::Low));
    let dim = frames(&mut scene, &mut gpu, &store, &mut stream, FRAMES + 1, 1);
    assert!(scene.apply_object_lighting(body, LightingMode::Restore));
    let restored = frames(&mut scene, &mut gpu, &store, &mut stream, FRAMES + 2, 1);

    let census = |a: &[u8], b: &[u8]| -> (usize, usize) {
        let mut up = 0;
        let mut down = 0;
        for i in 0..a.len() / 4 {
            let d = luma(&a[i * 4..i * 4 + 4]) - luma(&b[i * 4..i * 4 + 4]);
            if d > 8.0 {
                up += 1;
            } else if d < -8.0 {
                down += 1;
            }
        }
        (up, down)
    };
    let (b_up, b_down) = census(&bright, &c6);
    let (d_up, d_down) = census(&dim, &c7);
    let (r_up, r_down) = census(&restored, &c8);
    eprintln!("lighting: blink -- bright +{b_up}/-{b_down}, dim +{d_up}/-{d_down}, restored +{r_up}/-{r_down} pixels vs the control frames");
    assert!(b_up >= 100, "BRIGHT lifted only {b_up} pixels above the control -- the bright half still reads the baseline");
    assert!(
        b_down <= b_up / 20,
        "BRIGHT darkened {b_down} pixels (lifted {b_up})"
    );
    assert!(d_down >= 100, "DIM darkened only {d_down} pixels");
    assert!(
        d_up <= d_down / 20,
        "DIM brightened {d_up} pixels (darkened {d_down})"
    );
    assert!(
        r_up + r_down <= 8,
        "RESTORED is not back at the control: +{r_up}/-{r_down}"
    );
}
