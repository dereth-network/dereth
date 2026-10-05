//! The landscape draw distance: the `Render.LandscapeDrawDistance` preference (registered default
//! 8 = `Medium`, choices `[3, 5, 8, 11, 15, 25]`) sets the landscape window, the client's default
//! is 8, a window that reaches the far terrain paints the distant mountains, and a ridge beyond a
//! short window no longer hides an object behind it once the window reaches it.
//!
//! Fixture: the retail dats and a station recorded in retail at Holtburg, `/loc`
//! `0xA9B4002A [130.097656 24.053997 94.005005]` facing north, with an `ObjectStream` fed by hand
//! (no socket), on a software device. Radii 3 and 8 give 7x7 and 17x17 block windows whose
//! half-spans from the central block's centre are 672 m and 1632 m; the vertex census uses those
//! distances as reference bands, and the driven arms measure the larger window's effect on
//! terrain and occlusion.

#![cfg(gpu)]

use dereth_assets::world::CellLandblock;
use dereth_assets::Decode;
use dereth_client_model::weenie::{bitfield, item_type};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{LandblockId, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc, PublicWeenieDesc};
use dereth_render::device::Gpu;
use {
    dereth_client_runtime::character::CharacterInput,
    dereth_client_runtime::character::PLAYER_OBJECT_ID,
};
use {
    dereth_client_runtime::landblock::landblock_did, dereth_client_runtime::scene::SceneConfig,
    dereth_scene::world_scene::WorldScene,
};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

const W: u32 = 800;
const H: u32 = 600;

/// The station's landblock, Holtburg.
const HOLTBURG: u16 = 0xA9B4;
/// The recorded retail station's `/loc`, block-local.
// LINT-OK: `excessive_precision` — this is the recorded `/loc` transcribed digit for digit, and the
// point of the file is that it is that station and not one near it. Truncating it to what `f32` can
// hold would make the constant disagree with the evidence it names.
#[allow(clippy::excessive_precision)]
const STATION: (f32, f32) = (130.097_656, 24.053_997);

const BLOCK: f32 = 192.0;

/// The six landscape radii selected by the original overall graphics quality choices.
const DRAW_DISTANCE_CHOICES: [u32; 6] = [3, 5, 8, 11, 15, 25];
/// `Render.LandscapeDrawDistance`'s registered default — `Medium`.
const RETAIL_DEFAULT_MID_RADIUS: u32 = 8;

const TARGET: ObjectId = ObjectId(0x8300_0021);
/// A long-reach setup: 18 parts and no `GfxObjDegradeInfo` terminator, so it never goes dark
/// and a zero at range is occlusion rather than the degrade cutoff.
const NEVER_DARK: u32 = 0x0200_004D;

fn store() -> std::sync::Arc<RetailDatStore> {
    crate::common::dats()
}

// ---------------------------------------------------------------------------------------------
// The dat-only half: what is out there, and how far away it is.
// ---------------------------------------------------------------------------------------------

/// Every vertex height of one landblock, in metres, indexed `x * 9 + y`.
fn block_heights(store: &RetailDatStore, table: &[f32], bx: i32, by: i32) -> Option<[f32; 81]> {
    if !(0..255).contains(&bx) || !(0..255).contains(&by) {
        return None;
    }
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let lb = ((bx as u16) << 8) | (by as u16);
    let did = landblock_did(lb);
    let bytes = store.read_typed(DbType::LandBlock, did).ok()?;
    let lb = CellLandblock::decode_payload(did, &bytes).ok()?;
    let mut out = [0.0f32; 81];
    for (o, h) in out.iter_mut().zip(lb.height.iter()) {
        *o = table[*h as usize];
    }
    Some(out)
}

/// The terrain height at a **world** point, by the vertex grid alone (no interpolation — the
/// nearest vertex, which is all a silhouette census needs).
fn world_height(store: &RetailDatStore, table: &[f32], wx: f32, wy: f32) -> Option<f32> {
    let (bx, by) = (
        dereth_primitives::num::floor_to_i32(wx / BLOCK),
        dereth_primitives::num::floor_to_i32(wy / BLOCK),
    );
    let hs = block_heights(store, table, bx, by)?;
    #[allow(clippy::cast_precision_loss)]
    let (lx, ly) = (wx - bx as f32 * BLOCK, wy - by as f32 * BLOCK);
    let i = (lx / 24.0).round().clamp(0.0, 8.0);
    let j = (ly / 24.0).round().clamp(0.0, 8.0);
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    Some(hs[(i as usize) * 9 + (j as usize)])
}

/// **The premise.** In the northward vertex census, terrain in the 672–1632 m band stands
/// above the eye and above the highest sampled terrain in the nearer band. These are reference
/// distance bands, not an exact reconstruction of the off-centre station's window boundary.
///
/// Pure dat; no device, no scene.
#[test]
fn the_distant_mountains_are_outside_the_default_land_window() {
    let store = store();
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let table = &region.land_defs.land_height_table;

    let (bx, by) = dereth_client_runtime::landblock::block_xy(HOLTBURG);
    #[allow(clippy::cast_precision_loss)]
    let (wx, wy) = (bx as f32 * BLOCK + STATION.0, by as f32 * BLOCK + STATION.1);
    let eye = 94.005 + 2.54; // the station's z plus the chase camera's lift

    // Looking north, sample a 400 m wide fan out to 3 km.
    let mut inside = f32::MIN; // highest sampled ground in the near reference band
    let mut outside = f32::MIN; // highest sampled ground in the farther reference band
    let mut inside_at = 0.0f32;
    let mut outside_at = 0.0f32;
    let shipped_reach = 3.5 * BLOCK; // half-span of a radius-3 block window
    let retail_reach = 8.5 * BLOCK; // half-span of a radius-8 block window
    let mut d = 24.0f32;
    while d <= 3000.0 {
        let mut across = -200.0f32;
        while across <= 200.0 {
            if let Some(h) = world_height(&store, table, wx + across, wy + d) {
                if d <= shipped_reach {
                    if h > inside {
                        inside = h;
                        inside_at = d;
                    }
                } else if d <= retail_reach && h > outside {
                    outside = h;
                    outside_at = d;
                }
            }
            across += 24.0;
        }
        d += 24.0;
    }
    eprintln!(
        "draw distance premise: eye {eye:.1} m; highest ground within the shipped window {inside:.1} m at \
         {inside_at:.0} m; highest between {shipped_reach:.0} m and {retail_reach:.0} m is \
         {outside:.1} m at {outside_at:.0} m"
    );

    // The silhouette, band by band, over the whole forward fan: what a viewer at this eye would
    // see rising above the horizontal, and how far out it is. Printed rather than asserted,
    // because it is the thing to compare against the retail capture from this station.
    for (lo, hi) in [
        (0.0f32, 672.0f32),
        (672.0, 1632.0),
        (1632.0, 2208.0),
        (2208.0, 3000.0),
        (3000.0, 4000.0),
        (4000.0, 4896.0),
    ] {
        let mut peak = f32::MIN;
        let mut at = (0.0f32, 0.0f32);
        let mut d = lo.max(24.0);
        while d <= hi {
            let mut across = -d;
            while across <= d {
                if let Some(h) = world_height(&store, table, wx + across, wy + d) {
                    if h > peak {
                        peak = h;
                        at = (across, d);
                    }
                }
                across += 48.0;
            }
            d += 48.0;
        }
        let rise = math::atan2f(peak - eye, math::hypotf(at.1, at.0)).to_degrees();
        eprintln!(
            "draw distance silhouette: {lo:.0}-{hi:.0} m  peak {peak:6.1} m at ({:+.0}, {:.0})  {rise:5.2} deg above the eye",
            at.0, at.1
        );
    }
    assert!(
        outside > eye,
        "nothing beyond the shipped window stands above the eye, so the missing mountains are not \
         the window ({outside:.1} m vs {eye:.1} m)"
    );
    assert!(
        outside > inside,
        "the ground beyond the window ({outside:.1} m) is no higher than the ground inside it \
         ({inside:.1} m), so the window is not what hides it"
    );
}

/// **The preference's default, stated as a number.** `Render.LandscapeDrawDistance` registers
/// at 8. Radius 3 is the lowest menu choice, and the configured default must equal the
/// registered value rather than the lowest choice.
#[test]
fn the_shipped_window_is_the_lowest_of_the_six_choices() {
    use dereth_ui_screens::options::store as prefs;
    prefs::init();
    let registered = prefs::inq_value("Render.LandscapeDrawDistance");
    eprintln!("draw distance: Render.LandscapeDrawDistance registers as {registered:?}");
    assert_eq!(
        registered,
        Some(dereth_ui_screens::PrefValue::Int(
            i32::try_from(RETAIL_DEFAULT_MID_RADIUS).expect("8 fits")
        )),
        "the option store no longer registers retail's Medium default"
    );
    assert_eq!(
        DRAW_DISTANCE_CHOICES[0], 3,
        "VeryLow is 3; if this moved the sentence below is wrong"
    );
    let shipped = dereth_client_runtime::config::Config::default().land_radius;
    eprintln!("draw distance: the client's configured land_radius is {shipped}");
    assert_eq!(
        shipped, RETAIL_DEFAULT_MID_RADIUS,
        "the client's landscape draw distance is {shipped} where retail's registered default is \
         {RETAIL_DEFAULT_MID_RADIUS}"
    );
}

/// Behaviour: rendering.landscape.the-draw-distance-preference-sets-the-land-window
/// The preference reaches the configured radius rather than being decoration.
#[test]
fn the_landscape_draw_distance_preference_reaches_the_window() {
    use {dereth_client_runtime::config::Config, dereth_client_runtime::config::Preferences};
    for (label, radius) in [
        ("VeryLow", 3u32),
        ("Low", 5),
        ("Medium", 8),
        ("High", 11),
        ("VeryHigh", 15),
        ("Extreme", 25),
    ] {
        let p = Preferences::parse(&format!("[Render]\nLandscapeDrawDistance={label}\n"));
        let mut c = Config::default();
        c.apply_preferences(&p);
        assert_eq!(c.land_radius, radius, "{label}");
    }
    // Original preference parsing interprets an unmatched numeric label as an **index** into the
    // choice array, so a literal `3` in the file is `High` = 11.
    let p = Preferences::parse("[Render]\nLandscapeDrawDistance=3\n");
    let mut c = Config::default();
    c.apply_preferences(&p);
    assert_eq!(
        c.land_radius, 11,
        "a number in the file is an index, not a value"
    );
    // Nothing in the file leaves the registered default alone.
    let mut c = Config::default();
    c.apply_preferences(&Preferences::parse(""));
    assert_eq!(c.land_radius, RETAIL_DEFAULT_MID_RADIUS);
}

// ---------------------------------------------------------------------------------------------
// The driven half.
// ---------------------------------------------------------------------------------------------

struct Bench {
    gpu: Gpu,
    scene: WorldScene,
    store: std::sync::Arc<RetailDatStore>,
    objects: ObjectStream,
    now: f64,
    instance: u16,
}

impl Bench {
    fn new(store: &std::sync::Arc<RetailDatStore>, mut gpu: Gpu, land_radius: u32) -> Self {
        let region =
            dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
        let cfg = SceneConfig {
            landblock: HOLTBURG,
            land_radius,
            // Dusk, as retail's shot is. Lighting is held equal across every arm below so that
            // the differential is geometry and not the time of day.
            time_of_day: Some(0.78),
            particles: false,
            ..SceneConfig::default()
        };
        let mut scene = WorldScene::load(store, &mut gpu, cfg).expect("the scene loads");
        scene
            .attach_character(store, &region, &mut gpu)
            .expect("the body is created");
        let mut objects = ObjectStream::new();
        objects.world.player = Some(PLAYER_OBJECT_ID);
        let mut me = dereth_client_model::Weenie::new(PLAYER_OBJECT_ID);
        me.valid = true;
        me.has_phys_obj = true;
        me.qualities = Some(dereth_client_model::qualities::Qualities::new());
        objects.world.tables.weenies.insert(PLAYER_OBJECT_ID, me);
        Self {
            gpu,
            scene,
            store: store.clone(),
            objects,
            now: 1.0,
            instance: 0,
        }
    }

    fn outdoor(x: f32, y: f32, z: f32) -> Position {
        let mut cell = LandblockId(HOLTBURG).cell(1);
        let mut o = Vec3::new(x, y, z);
        dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut o);
        Position::new(cell, dereth_primitives::Frame::new(o, Quat::IDENTITY))
    }

    fn height(&self, x: f32, y: f32) -> Option<f32> {
        self.scene
            .character
            .as_ref()?
            .world
            .terrain_height_at(&Self::outdoor(x, y, 0.0))
    }

    /// Stand at the recorded station, facing north as it does.
    fn stand_at_the_recorded_station(&mut self) {
        let z = self
            .height(STATION.0, STATION.1)
            .expect("the recorded station is on terrain");
        let mut p = Self::outdoor(STATION.0, STATION.1, z + 0.01);
        p.frame.rotation = Quat::IDENTITY; // yaw 0 = north
        self.scene.character.as_mut().expect("a body").teleport(p);
    }

    /// `scale` supplies `PhysicsDesc.object_scale`, setting the body's size. A distant object needs it:
    /// an unscaled body at a kilometre is two or three pixels, and a count that small cannot tell
    /// occlusion from rounding.
    fn place(&mut self, at: Position, setup: u32, scale: f32) {
        // **The block the object is created into is made resident first, the same prerequisite
        // every create and every teleport in this client states.**
        //
        // `app::apply_player_teleport_at` opens with `character.land().load_block_cells(block)`
        // before `Character::teleport`, `Character::new` does it before the body is placed, and
        // `WorldScene::stream` does it for every block the window touches. Visible-cell lookup
        // checks residency, so a block that is only *decoded* resolves **no cells**:
        // `enter_world`'s placement finds none, `ObjectPhysics` records the body as `unplaced`,
        // `finish_object_physics` sets `SceneObject::position` to `None`, and the object is never
        // submitted. A target beyond the short window would otherwise never be placed, and the
        // premise assertion "the object is submitted in every arm" would catch it, because a zero
        // from an unplaced body is not occlusion.
        //
        // The subject is the **draw** window, so residency is a precondition the bench supplies
        // rather than a variable it measures: this line leaves the drawn terrain radius exactly as
        // each arm sets it, and only lets the body exist. It is not a claim that a server would
        // send a create a kilometre away.
        if let Some(land) = self
            .scene
            .character
            .as_ref()
            .map(|c| std::sync::Arc::clone(c.land()))
        {
            land.load_block_cells(at.cell.landblock());
        }
        self.instance += 1;
        let payload = dereth_protocol::objects::ObjectCreatePayload {
            id: TARGET,
            objdesc: ObjDesc::default(),
            physicsdesc: PhysicsDesc {
                bitfield: flags::POSITION | flags::SETUP | flags::OBJSCALE,
                object_scale: Some(scale),
                setup_id: Some(setup),
                state: 0,
                position: Some(dereth_protocol::types::PositionWire {
                    objcell_id: at.cell.0,
                    frame: dereth_protocol::types::Frame {
                        origin: at.frame.origin.into(),
                        orientation: Quat::IDENTITY.into(),
                    },
                }),
                timestamps: dereth_protocol::types::PhysicsTimestamps {
                    instance: self.instance,
                    ..dereth_protocol::types::PhysicsTimestamps::default()
                },
                ..PhysicsDesc::default()
            },
            wdesc: PublicWeenieDesc::default(),
        };
        let body =
            dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
                .expect("encode");
        self.objects.apply_event(
            &SessionEvent::WorldObject {
                opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
                body,
            },
            LocalTime(self.now),
        );
        {
            let Self {
                store,
                objects,
                scene,
                ..
            } = self;
            let ch = scene.character.as_mut().expect("a body");
            objects.sync_physics(store, &mut ch.world);
        }
        let w = self
            .objects
            .world
            .tables
            .weenies
            .get_mut(TARGET)
            .expect("placed");
        w.pwd.obj_type |= item_type::CREATURE;
        w.pwd.bitfield |= bitfield::ATTACKABLE;
        w.pwd.radar_enum = Some(4);
        self.objects.world.update_visible_object_list();
    }

    fn draw(&mut self) -> Vec<u8> {
        self.now += 1.0;
        let Self {
            store,
            gpu,
            scene,
            objects,
            ..
        } = self;
        scene
            .sync_objects(store, gpu, objects)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(self.now),
            1.0 / 30.0,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
        gpu.capture().expect("capture").to_rgba()
    }

    fn settle(&mut self) -> Vec<u8> {
        let mut f = Vec::new();
        for _ in 0..12 {
            f = self.draw();
        }
        f
    }

    fn submitted(&self) -> usize {
        self.scene
            .drawn_part_order()
            .iter()
            .filter(|e| e.object == Some(TARGET))
            .count()
    }
}

/// Pixels that differ between two RGBA frames.
fn differing(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(p, q)| p != q)
        .count()
}

/// **The frame.** Reproduce the recorded station and measure how much of it the window reaches.
///
/// The observable is the number of changed pixels in the *upper half* of the frame, where the
/// far terrain meets the sky at this station. The radius-3 and radius-8 captures must differ by
/// more than 2,000 pixels there. This counts the window's effect; it does not identify each pixel
/// as belonging to a particular distance band.
#[test]
fn the_recorded_frame_loses_the_far_terrain_to_the_window() {
    let store = store();
    let gpu = crate::common::test_gpu(W, H);
    let mut near = Bench::new(&store, gpu, 3);
    near.stand_at_the_recorded_station();
    let shipped = near.settle();
    drop(near);

    let gpu = crate::common::test_gpu(W, H);
    let mut far = Bench::new(&store, gpu, RETAIL_DEFAULT_MID_RADIUS);
    far.stand_at_the_recorded_station();
    let retail = far.settle();

    let changed = differing(&shipped, &retail);
    // Where the far terrain lands: the band above the horizon-ish middle of the frame.
    let row = |f: &[u8], y: u32| -> usize {
        let s = (y * W * 4) as usize;
        f[s..s + (W * 4) as usize]
            .chunks_exact(4)
            .zip(retail[s..s + (W * 4) as usize].chunks_exact(4))
            .filter(|(p, q)| p != q)
            .count()
    };
    let mut upper = 0usize;
    for y in 0..H / 2 {
        upper += row(&shipped, y);
    }
    eprintln!(
        "draw distance frame: land_radius 3 vs {RETAIL_DEFAULT_MID_RADIUS} at the recorded station differs in \
         {changed} of {} pixels, {upper} of them above the middle of the frame",
        W * H
    );
    if let Ok(dir) = std::env::var("DERETH_TEST_LANDSCAPE_DRAW_DISTANCE_DUMP") {
        let _ = std::fs::write(format!("{dir}/landscape-radius3.rgba"), &shipped);
        let _ = std::fs::write(format!("{dir}/landscape-radius8.rgba"), &retail);
    }
    assert!(
        upper > 2_000,
        "the shipped window and retail's paint the same distance: only {upper} pixels above the \
         middle of the frame differ"
    );
    drop(far);

    // **The rejecting half.** The same frame under the radius the client chooses for itself,
    // against the same frame under retail's registered default: the two arms are the same window
    // and the frame is identical. A client defaulting to radius 3 differs in thousands of pixels.
    let configured = dereth_client_runtime::config::Config::default().land_radius;
    let gpu = crate::common::test_gpu(W, H);
    let mut own = Bench::new(&store, gpu, configured);
    own.stand_at_the_recorded_station();
    let ours = own.settle();
    let gap = differing(&ours, &retail);
    eprintln!(
        "draw distance frame: the client's own land_radius is {configured}; its frame differs from retail's \
         registered default in {gap} pixels"
    );
    assert_eq!(
        gap, 0,
        "the client draws the recorded station at land_radius {configured}, not at retail's \
         {RETAIL_DEFAULT_MID_RADIUS}: {gap} pixels of far terrain are missing"
    );
}

/// **The rejecting test.** Reproduce the short window's missing ridge, then compare it
/// with the registered default window and the window the client currently configures for itself.
///
/// The positive control is the same object at the same bearing with the same lighting under a
/// window that reaches the ridge. The object must be *submitted* in every arm; the far arm must
/// reduce its pixel count by at least eight, not necessarily hide it completely. Lifted and buried
/// controls distinguish visibility at that range from terrain leakage.
#[test]
fn an_object_behind_a_ridge_beyond_the_window_draws_through_it() {
    let store = store();
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    let table = &region.land_defs.land_height_table;
    let (bx, by) = dereth_client_runtime::landblock::block_xy(HOLTBURG);
    #[allow(clippy::cast_precision_loss)]
    let (wx, wy) = (bx as f32 * BLOCK + STATION.0, by as f32 * BLOCK + STATION.1);
    let eye = 94.005 + 2.54;

    // Search the forward cone using the fixed 3.5-block (672 m) near reference band and a
    // farther band ending at 7 blocks, inside the radius-8 window. Seek a crest beyond the near
    // band which covers farther ground while the near-band terrain does not already cover it.
    // These bands select a candidate; the drawn controls below test actual window occlusion.
    //
    // Compare slopes above the eye, the crest's `(h - eye) / d` against the target's.
    // The station has to satisfy **both** halves or the measurement says nothing:
    //
    // * the target's ground must stand above the sampled near-band silhouette; and
    // * a farther sampled crest must stand above that ground.
    //
    // `clear` is the first slope margin and `cover` the second. Slopes approximate angles in
    // radians at small elevations, but no arctangent is taken here. Multiplying `cover` by range
    // gives the available height; the object's scale is chosen from that height below.
    let mut best: Option<(f32, f32, f32, f32, f32, f32)> = None;
    let mut bearing = -25.0f32;
    while bearing <= 25.0 {
        let (s, c) = math::sin_cosf(bearing.to_radians());
        let sample = |d: f32| world_height(&store, table, wx + s * d, wy + c * d);
        let (mut inside_angle, mut crest_angle, mut crest_at) = (f32::MIN, f32::MIN, 0.0f32);
        let mut d = 24.0f32;
        while d <= 3.5 * BLOCK {
            if let Some(h) = sample(d) {
                inside_angle = inside_angle.max((h - eye) / d);
            }
            d += 24.0;
        }
        // Keep the target short of the far window's last row. Submission is checked separately
        // in every arm so missing residency cannot masquerade as occlusion.
        while d <= 7.0 * BLOCK {
            if let Some(h) = sample(d) {
                let a = (h - eye) / d;
                if a > crest_angle {
                    crest_angle = a;
                    crest_at = d;
                }
                let clear = a - inside_angle;
                let cover = crest_angle - a;
                if d > crest_at && clear > 0.004 && cover > 0.012 {
                    let score = clear.min(cover);
                    let better = best.is_none_or(|(_, _, _, bs2, _, _)| score > bs2);
                    if better {
                        best = Some((bearing, d, h, score, crest_at, cover));
                    }
                }
            }
            d += 24.0;
        }
        bearing += 2.5;
    }
    let (bearing, td, th, score, crest_at, cover) =
        best.expect("no crest outside the shipped window buries anything inside retail's");
    // How tall the object may be and still sit under the crest, with a third of the clearance
    // kept back for the difference between this 24 m vertex grid and the drawn mesh.
    let headroom = cover * td * (2.0 / 3.0);
    eprintln!(
        "draw distance ridge: bearing {bearing:+.1} deg, crest at {crest_at:.0} m (outside the shipped \
         window's {:.0} m), target ground {th:.1} m at {td:.0} m; clear of the near silhouette \
         and under the crest by {score:.4} rad, headroom {headroom:.1} m",
        3.5 * BLOCK
    );
    let (bs, bc) = math::sin_cosf(bearing.to_radians());
    let at = Bench::outdoor(STATION.0 + bs * td, STATION.1 + bc * td, th + 1.0);
    // `PhysicsDesc.object_scale`, chosen so the object fills the headroom: an unscaled 2 m body a
    // kilometre away is two or three pixels, and a count that small cannot tell occlusion from
    // rounding. `CSetup 0x0200004D` is about 2 m tall, so the scale is the headroom in metres
    // halved, capped where the body would outgrow the crest.
    let scale = (headroom / 2.0).clamp(1.0, 12.0);
    let burial = cover * td;

    // Three arms, identical but for `land_radius`: the short window, retail's registered
    // default, and **the window this client actually configures itself with**. The first two are
    // the measurement that the window is what decides it.
    // Two benches per arm, drawn in lockstep, so that the sky's own animation cancels: a "before
    // and after in one bench" differential counts the clouds.
    let configured = dereth_client_runtime::config::Config::default().land_radius;
    let mut painted = Vec::new();
    let mut submits = Vec::new();
    let mut control = 0usize;
    let mut floor = 0usize;
    for (i, radius) in [3u32, RETAIL_DEFAULT_MID_RADIUS, configured]
        .into_iter()
        .enumerate()
    {
        let g = crate::common::test_gpu(W, H);
        let mut a = Bench::new(&store, g, radius);
        let g = crate::common::test_gpu(W, H);
        let mut b = Bench::new(&store, g, radius);
        a.stand_at_the_recorded_station();
        b.stand_at_the_recorded_station();
        for _ in 0..12 {
            a.draw();
            b.draw();
        }
        b.place(at, NEVER_DARK, scale);
        let (mut fa, mut fb) = (Vec::new(), Vec::new());
        for _ in 0..3 {
            fa = a.draw();
            fb = b.draw();
        }
        painted.push(differing(&fa, &fb));
        submits.push(b.submitted());
        if i == 1 {
            // **The positive control, and it is the one that makes a zero mean anything.** The
            // same object, the same range and the same window, lifted clear of the crest: if it
            // paints nothing here either, the zero above is the horizon, the view cone or the
            // degrade cutoff rather than the terrain.
            b.place(
                Bench::outdoor(
                    STATION.0 + bs * td,
                    STATION.1 + bc * td,
                    th.max(eye) + burial + 40.0,
                ),
                NEVER_DARK,
                scale,
            );
            for _ in 0..3 {
                fa = a.draw();
                fb = b.draw();
            }
            control = differing(&fa, &fb);
            // **The floor.** The same object at the same bearing and range, sunk 60 m *under* the
            // terrain, where no silhouette argument can put a single pixel of it on screen. Any
            // residue here measures leakage through the terrain (a missing transition adjustment
            // at the ring-2 and ring-4 LOD boundaries shows here); read the occluded arms against
            // the measured floor rather than assuming zero leakage.
            b.place(
                Bench::outdoor(STATION.0 + bs * td, STATION.1 + bc * td, th - 60.0),
                NEVER_DARK,
                scale,
            );
            for _ in 0..3 {
                fa = a.draw();
                fb = b.draw();
            }
            floor = differing(&fa, &fb);
        }
    }
    eprintln!(
        "draw distance occlusion: radius 3 submits {} subsets and paints {} pixels; radius \
         {RETAIL_DEFAULT_MID_RADIUS} submits {} and paints {}; the configured radius {configured} \
         submits {} and paints {}. Controls at radius {RETAIL_DEFAULT_MID_RADIUS}: lifted clear of \
         the crest {control}, sunk 60 m under the ground {floor}",
        submits[0], painted[0], submits[1], painted[1], submits[2], painted[2]
    );
    assert!(
        submits.iter().all(|s| *s > 0),
        "the object is not submitted in one of the arms, so a zero would not be occlusion"
    );
    // The two controls bracket the measurement. Without the first, "few pixels" could mean an
    // object that never draws at this range at all; without the second, it could mean terrain
    // that leaks whatever is behind it.
    assert!(
        control > 3 * floor.max(1),
        "lifted clear of the crest the object paints {control} pixels against a leak floor of \
         {floor}; the two are not far enough apart to read anything from"
    );
    // The crest is what makes the difference: with it drawn, the object loses a measurable part
    // of itself. Complete occlusion is not required: with roughly 8 m of headroom against an
    // 8 m object the crown can clear the crest, and the buried control can show a handful of
    // pixels. The residual and the floor are reported rather than assumed zero.
    assert!(
        painted[0] >= painted[1] + 8,
        "drawing the crest changes the object from {} pixels to {} — too little to read as \
         occlusion",
        painted[0],
        painted[1]
    );
    // **The rejecting assertion**: what the client configures for itself, unprompted, against
    // what retail's registered default does.
    assert_eq!(
        painted[2], painted[1],
        "the client's own landscape draw distance ({configured}) does not occlude this object the \
         way retail's {RETAIL_DEFAULT_MID_RADIUS} does: {} pixels against {}",
        painted[2], painted[1]
    );
}
