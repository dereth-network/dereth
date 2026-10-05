//! Scene lighting affects a particle with zero surface luminosity: it is drawn at
//! `saturate(0 + ambient + sum_k ...)` of its texture, not at full brightness.
//!
//! Particle objects and their parts draw by the ordinary part/mesh/subset route: with sunlight
//! disabled a mesh selects local lights by its drawing sphere, subset drawing enables
//! fixed-function lighting unconditionally, and a surface luminosity at or below zero leaves the
//! material's default zero emissive and unit diffuse/ambient in place. Particle drawing binds the
//! same material coefficients and per-card light set when object lighting is enabled.
//!
//! In the shipped dats about 86 of 268 distinct particle surfaces are non-luminous, referenced by
//! about 140 of 2,051 emitters; the metadata test requires more than 100 emitters and 50 surfaces.
//! The selected fixture is stricter: all ten CreateParticle hooks use mesh 0x01001BBE, whose
//! surfaces must have exactly zero luminosity and type 0x10102 (BASE1_IMAGE | ALPHA | ADDITIVE).
//!
//! In the academy room of the interior lighting station, construct a host and optionally play
//! script 0x3300088E. Four fresh runs produce final images: played/unplayed with object lighting enabled, then played/unplayed
//! with it disabled. Within each lighting setting, subtraction is intended to cancel the stable
//! scene and host. SRCALPHA/ONE makes an unsaturated additive contribution recoverable by that
//! difference. The runs do not independently assert that every background pixel stayed equal.
//!
//! A common mask drops an entire pixel if any RGB channel in any of the four images is >= 250.
//! Remaining signed channel differences are summed, and each lit sum is divided by its unlit
//! counterpart. The test requires a substantial total unlit signal and more than 500 lit pixels
//! with some positive channel difference > 2; it does not separately gate each denominator.
//!
//! Compare each ratio with an approximate host-site bracket: `saturate(ambient)` through
//! `saturate(ambient + sum_k Ld_k/d_k)`, using the current scene's selected lights and 0.06 slack.
//! The fixed-function equation has Me=0, Ma=Md=1 and N.L in [0,1]. The helper uses the
//! host origin/radius 0.5; actual cards have their own positions and drawing-sphere light sets.
//! It does not reconstruct every billboard normal or establish a strict image-ratio bound.
//!
//! The far-site ceiling 0.65 is separately measured, not derived from ambient alone. The station
//! measures about 0.39 there, while a particle drawn without lighting reads 1.000 at both sites.
//! Strong local lights (the room's statics have intensity 100) can legitimately saturate the
//! fixed-function sum. The near-site test requires its mean ratio to exceed the far site's
//! by a factor of 1.5; it does not assert a final value of 1.000.
//!
//! No datagrams are sent. Retail DATs and a software GPU are required; device creation failure
//! is a hard failure here. Images come from the current renderer, not a stored image golden.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::Decode;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::camera::CameraInput;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::num::math;
use dereth_primitives::{CellId, DataId, Frame, LocalTime, ObjectId, Position, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::{PhysicsDesc, PublicWeenieDesc};
use dereth_protocol::Message;
use dereth_render::device::Gpu;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

const W: u32 = 800;
const H: u32 = 600;

/// The interior lighting station's room and position. Interior lighting uses white ambient at
/// level 0.2 while the viewer occupies a cell not seen from outside; world_ambient_color has that
/// branch, and this test queries it for the bracket rather than asserting a fixed ambient array.
const STATION_CELL: CellId = CellId(0x8602_01B1);
const STATION_ORIGIN: Vec3 = Vec3::new(18.061_283, -30.180_618, 0.005);
/// The station's recorded heading `0.115688 0 0 -0.993286`, expressed here as a level bearing
/// looking down the room towards the hearth.
const STATION_BEARING_DEGREES: f32 = 166.72;

/// Public script 0x3300088E has ten CreateParticle hooks and no other steps, all using mesh
/// 0x01001BBE with exactly zero-luminosity additive surfaces. The metadata test verifies those
/// relationships. It is setup 0x020007E6's script; the host below explicitly plays it rather
/// than using that setup's default script.
const LUM0_SCRIPT: DataId = DataId(0x3300_088E);
/// The mesh those ten hooks draw.
const LUM0_MESH: DataId = DataId(0x0100_1BBE);
/// `BASE1_IMAGE | ALPHA | ADDITIVE`.
const ADDITIVE: u32 = 0x0001_0102;

/// Shipped setup 0x02000177 was selected because its default script produces no particles.
/// The same host is constructed in every run so stable host geometry can cancel within each
/// played/unplayed pair. The fixture test does not independently verify that default-script
/// property. This is also the setup used by the script-play test.
const HOST_SETUP: u32 = 0x0200_0177;
const HOST_ID: ObjectId = ObjectId(0x8000_89A0);

/// Sites 3.0 m and 8.5 m horizontally in front of the body, both 1.2 m above its feet. The near
/// site is about 2.3 m from a room static and 3 m from the white viewer light (intensity 2.25,
/// falloff 10), while the far site receives weaker, more oblique light. These distances to lights
/// are observations, not separate assertions.
const NEAR_METRES: f32 = 3.0;
const FAR_METRES: f32 = 8.5;
const EYE_HEIGHT: f32 = 1.2;

/// Level-bearing quaternion with forward axis `(sin d, cos d, 0)`, matching the heading
/// convention used by the station's recorded position command.
fn heading_quat(degrees: f32) -> Quat {
    let h = degrees.to_radians() / 2.0;
    Quat::new(math::cosf(h), 0.0, 0.0, -math::sinf(h))
}

/// Drop the whole pixel from both sums if any RGB channel in any of the four frames reaches
/// this byte threshold; alpha does not participate in the mask.
const SATURATED: u8 = 250;

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn base_cfg() -> SceneConfig {
    SceneConfig {
        landblock: 0x8602,
        start_cell: Some(STATION_CELL),
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    }
}

/// Encode an ItemCreateObject body and feed it directly to ObjectStream's WorldObject event
/// decoder with POSITION and SETUP flags. This exercises the body codec and object application,
/// without socket framing, session transport or server-side creation.
fn spawn(stream: &mut ObjectStream, cell: CellId, origin: Vec3) {
    let payload = ObjectCreatePayload {
        id: HOST_ID,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(HOST_SETUP),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: cell.0,
                frame: dereth_protocol::types::Frame {
                    origin: origin.into(),
                    orientation: Quat::IDENTITY.into(),
                },
            }),
            ..Default::default()
        },
        wdesc: PublicWeenieDesc::default(),
    };
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: ItemCreateObject::OPCODE,
            body: dereth_protocol::write_body(&ItemCreateObject(payload)).unwrap(),
        },
        LocalTime(0.0),
    );
}

/// `metres` in front of the body, at eye height. Derived from the station pose rather than
/// written down, so the two arms of a pair cannot drift apart, and **horizontal** so that the
/// chase camera's downward pitch cannot put the emitter through the floor.
fn site_at(bearing: f32, metres: f32) -> Vec3 {
    let mut f = Frame::new(STATION_ORIGIN, heading_quat(bearing));
    f.origin = Vec3::ZERO;
    let fwd = dereth_physics::math::localtoglobal(&f, Vec3::new(0.0, 1.0, 0.0));
    let n = (fwd.x * fwd.x + fwd.y * fwd.y).sqrt().max(1e-6);
    Vec3::new(
        STATION_ORIGIN.x + fwd.x / n * metres,
        STATION_ORIGIN.y + fwd.y / n * metres,
        STATION_ORIGIN.z + EYE_HEIGHT,
    )
}

/// Fresh scene at the recorded pose: 180 minimum-quantum updates, constructing the host at
/// index 20 and playing the script at index 40 only for the played arm. Sync/update/stream/draw
/// every iteration and capture the last one, which is the image returned. Each pair member starts
/// anew.
fn walk(
    store: &Arc<RetailDatStore>,
    gpu: &mut Gpu,
    cfg: SceneConfig,
    bearing: f32,
    metres: f32,
    play: bool,
) -> (WorldScene, Vec3, Vec<u8>) {
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let mut scene = WorldScene::load(store, gpu, cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, gpu)
        .expect("the body is created");
    {
        let c = scene.character.as_mut().expect("a body");
        c.teleport(Position::new(
            STATION_CELL,
            Frame::new(STATION_ORIGIN, heading_quat(bearing)),
        ));
    }
    scene.follow_character_now();
    let mut stream = ObjectStream::new();
    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    let mut site = Vec3::ZERO;
    for i in 0..180 {
        now += step;
        if i == 20 {
            site = site_at(bearing, metres);
            spawn(&mut stream, STATION_CELL, site);
        }
        if i == 40 && play {
            assert!(
                scene.play_script_id(HOST_ID, LUM0_SCRIPT),
                "the shipped PES plays"
            );
        }
        scene
            .sync_objects(store, gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            step,
        );
        scene.stream(store, gpu).expect("stream");
        scene.reserve_upload_arena(gpu).expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    let rgba = gpu.capture().expect("capture").to_rgba();
    (scene, site, rgba)
}

/// The four frames of one site: `(lit played, lit not played, unlit played, unlit not played)`.
struct Pair {
    scene: WorldScene,
    site: Vec3,
    on: Vec<u8>,
    off: Vec<u8>,
    unlit_on: Vec<u8>,
    unlit_off: Vec<u8>,
}

/// Signed played-minus-unplayed RGB sums after the common mask, plus the reported bounding
/// box and count of pixels with a channel difference > 2. Negative differences remain in the
/// energy sums. Dropped counts cover all saturated pixels, not just the emitter's footprint.
struct Energy {
    rgb: [f64; 3],
    bbox: (usize, usize, usize, usize),
    touched: usize,
    dropped: usize,
}

fn contribution(p: &Pair, lit: bool) -> Energy {
    let (on, off) = if lit {
        (&p.on, &p.off)
    } else {
        (&p.unlit_on, &p.unlit_off)
    };
    let (mut x0, mut y0, mut x1, mut y1) = (W as usize, H as usize, 0usize, 0usize);
    let mut rgb = [0.0f64; 3];
    let (mut touched, mut dropped) = (0usize, 0usize);
    for y in 0..H as usize {
        for x in 0..W as usize {
            let at = (y * W as usize + x) * 4;
            // One mask for both arms: a pixel any arm clipped carries no usable difference.
            let sat = [&p.on, &p.off, &p.unlit_on, &p.unlit_off]
                .iter()
                .any(|f| f[at..at + 3].iter().any(|&c| c >= SATURATED));
            if sat {
                dropped += 1;
                continue;
            }
            let mut any = false;
            for c in 0..3 {
                let d = f64::from(on[at + c]) - f64::from(off[at + c]);
                rgb[c] += d;
                any |= d > 2.0;
            }
            if any {
                touched += 1;
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    Energy {
        rgb,
        bbox: (x0, y0, x1 + 1, y1 + 1),
        touched,
        dropped,
    }
}

/// Host-site bracket derived from the fixed-function vertex-color equation:
///
/// ```text
/// diffuse = saturate(Me + Ma * D3DRS_AMBIENT + sum_k Md * Ld_k * max(N . L_k, 0) * atten_k)
/// ```
///
/// Default Me=0 and Ma=Md=1, with white vertex color supplying the defaults when no material
/// clone is bound. A non-positive luminosity leaves emissive unchanged. Local lights use
/// attenuation (0,1,0), giving 1/d inside Range. Bounding N.L between 0 and 1 removes the need
/// to reproduce each billboard's normal. Positions are compared in renderer `(x,z,y)` space.
///
/// Query the current scene's ambient and light-selection implementation at the host origin and
/// supplied radius, skip lights at/beyond range, guard distance by 1e-6 and clamp both endpoints
/// to 1. Cards occupy a volume and select lights individually, so the image check adds slack;
/// this helper is not an independent light selector or exact per-card image oracle.
fn fixed_function_bracket(scene: &WorldScene, p: Vec3, radius: f32) -> ([f64; 3], [f64; 3]) {
    let amb = scene.world_ambient_color();
    let swap = |v: Vec3| Vec3::new(v.x, v.z, v.y);
    let ps = swap(p);
    let mut lo = [0.0f64; 3];
    let mut hi = [0.0f64; 3];
    for c in 0..3 {
        lo[c] = f64::from(amb[c]);
        hi[c] = f64::from(amb[c]);
    }
    for l in &scene.object_light_set(p, radius, false) {
        let d = Vec3::new(
            l.position[0] - ps.x,
            l.position[1] - ps.y,
            l.position[2] - ps.z,
        );
        let dist = (d.x * d.x + d.y * d.y + d.z * d.z).sqrt();
        if dist >= l.range {
            continue;
        }
        for c in 0..3 {
            hi[c] += f64::from(l.diffuse[c]) / f64::from(dist.max(1e-6));
        }
    }
    for c in 0..3 {
        lo[c] = lo[c].min(1.0);
        hi[c] = hi[c].min(1.0);
    }
    (lo, hi)
}

fn pair(store: &Arc<RetailDatStore>, gpu: &mut Gpu, metres: f32) -> Pair {
    let mut unlit = base_cfg();
    unlit.object_lighting = false;
    let b = STATION_BEARING_DEGREES;
    let (scene, site, on) = walk(store, gpu, base_cfg(), b, metres, true);
    let (_, _, off) = walk(store, gpu, base_cfg(), b, metres, false);
    let (_, _, unlit_on) = walk(store, gpu, unlit.clone(), b, metres, true);
    let (_, _, unlit_off) = walk(store, gpu, unlit, b, metres, false);
    Pair {
        scene,
        site,
        on,
        off,
        unlit_on,
        unlit_off,
    }
}

// ---------------------------------------------------------------------------------------------
// 0. The fixture is the shipped data, not this file's belief about it.
// ---------------------------------------------------------------------------------------------

/// Verify the selected script/hooks/mesh/surfaces and count readable, decodable emitter-surface
/// references. This does not verify every spatial/material assumption of the image fixture.
#[test]
fn the_fixture_is_what_this_file_claims() {
    let store = store();

    // The script: ten steps, every one a CreateParticle, every emitter drawing LUM0_MESH.
    let b = store
        .read_typed(DbType::PhysicsScript, LUM0_SCRIPT)
        .expect("shipped");
    let ps = dereth_assets::PhysicsScript::decode_payload(LUM0_SCRIPT, &b).expect("decodes");
    let hooks: Vec<DataId> = ps
        .script_data
        .iter()
        .filter_map(|s| match &s.hook.data {
            dereth_assets::hook::HookData::CreateParticle {
                emitter_info_id, ..
            } => Some(*emitter_info_id),
            _ => None,
        })
        .collect();
    assert_eq!(hooks.len(), 10, "the script's CreateParticle hooks");
    assert_eq!(
        ps.script_data.len(),
        hooks.len(),
        "the script does nothing else"
    );
    for e in &hooks {
        let eb = store
            .read_typed(DbType::ParticleEmitter, *e)
            .expect("shipped emitter");
        let info = dereth_assets::ParticleEmitterInfo::decode_payload(*e, &eb).expect("decodes");
        assert_eq!(info.hw_gfxobj_id, LUM0_MESH, "{e:?} draws another mesh");
    }

    // The mesh's surfaces: all additive, all luminosity 0.
    let gb = store
        .read_typed(DbType::GfxObj, LUM0_MESH)
        .expect("shipped mesh");
    let g = dereth_assets::GfxObj::decode_payload(LUM0_MESH, &gb).expect("decodes");
    assert!(!g.surfaces.is_empty(), "the mesh has no surface");
    for sid in &g.surfaces {
        let sb = store
            .read_typed(DbType::Surface, *sid)
            .expect("shipped surface");
        let s = dereth_assets::Surface::decode_payload(*sid, &sb).expect("decodes");
        assert_eq!(
            s.luminosity, 0.0,
            "{sid:?} is luminous -- the wrong fixture"
        );
        assert_eq!(s.surface_type, ADDITIVE, "{sid:?} is not SRCALPHA/ONE");
    }

    // Scan non-positive luminosity, skipping unreadable/undecodable emitters, meshes and
    // surfaces. Emitter counts precede mesh decoding; distinct surface IDs are deduplicated.
    let mut lum0_surfaces = 0usize;
    let mut surfaces = std::collections::BTreeSet::new();
    let mut emitters = 0usize;
    let mut emitters_lum0 = 0usize;
    for id in store.ids_of(DbType::ParticleEmitter) {
        let Ok(b) = store.read_typed(DbType::ParticleEmitter, id) else {
            continue;
        };
        let Ok(e) = dereth_assets::ParticleEmitterInfo::decode_payload(id, &b) else {
            continue;
        };
        emitters += 1;
        let Ok(gb) = store.read_typed(DbType::GfxObj, e.hw_gfxobj_id) else {
            continue;
        };
        let Ok(g) = dereth_assets::GfxObj::decode_payload(e.hw_gfxobj_id, &gb) else {
            continue;
        };
        let mut any0 = false;
        for sid in &g.surfaces {
            let Ok(sb) = store.read_typed(DbType::Surface, *sid) else {
                continue;
            };
            let Ok(s) = dereth_assets::Surface::decode_payload(*sid, &sb) else {
                continue;
            };
            if s.luminosity <= 0.0 {
                any0 = true;
                if surfaces.insert(sid.0) {
                    lum0_surfaces += 1;
                }
            } else {
                surfaces.insert(sid.0);
            }
        }
        emitters_lum0 += usize::from(any0);
    }
    eprintln!(
        "particle census: {emitters} shipped emitters, {emitters_lum0} of them referencing one of {lum0_surfaces} non-positive-luminosity surfaces out of {} distinct particle surfaces",
        surfaces.len()
    );
    assert!(
        emitters_lum0 > 100 && lum0_surfaces > 50,
        "the lum-0 particle population collapsed: {emitters_lum0} emitters, {lum0_surfaces} surfaces"
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The station.
// ---------------------------------------------------------------------------------------------

/// Behaviour: rendering.particles.a-non-luminous-particle-is-lit-by-the-scene
/// Compare final-frame differential ratios at two sites against the host-site bracket, the
/// measured far ceiling and the near/far mean-rise control. These are selected fixture gates,
/// not coverage of every non-luminous emitter counted above.
///
/// A particle drawn without its material lighting and light parameters matches its unlit card,
/// a factor of 1.000 at both sites; the unlit arm reproduces that missing-lighting behaviour.
#[test]
fn a_non_luminous_particle_is_lit_by_the_scene() {
    let store = store();
    let mut gpu = crate::common::test_gpu(W, H);

    let mut rows: Vec<(&str, [f64; 3], [f64; 3], [f64; 3])> = Vec::new();
    for (name, metres) in [("far", FAR_METRES), ("near", NEAR_METRES)] {
        let p = pair(&store, &mut gpu, metres);
        let lit = contribution(&p, true);
        let unlit = contribution(&p, false);
        let (lo, hi) = fixed_function_bracket(&p.scene, p.site, 0.5);
        let amb = p.scene.world_ambient_color();
        let lights = p.scene.object_light_set(p.site, 0.5, false);
        for l in &lights {
            let d = [
                l.position[0] - p.site.x,
                l.position[1] - p.site.z,
                l.position[2] - p.site.y,
            ];
            let dist = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            eprintln!(
                "particle {name:<5}   light type {} diffuse {:?} range {:.2} dist {dist:.2}",
                l.light_type, l.diffuse, l.range
            );
        }
        let factor = [
            lit.rgb[0] / unlit.rgb[0],
            lit.rgb[1] / unlit.rgb[1],
            lit.rgb[2] / unlit.rgb[2],
        ];
        eprintln!(
            "particle {name:<5} site ({:.2},{:.2},{:.2}) ambient {amb:?} lights {} \
             box {:?} touched {} dropped {}\n\
             particle {name:<5} unlit {:.0}/{:.0}/{:.0} lit {:.0}/{:.0}/{:.0} \
             factor {:.3}/{:.3}/{:.3} bracket {:.3}..{:.3} / {:.3}..{:.3} / {:.3}..{:.3}",
            p.site.x,
            p.site.y,
            p.site.z,
            lights.len(),
            lit.bbox,
            lit.touched,
            lit.dropped,
            unlit.rgb[0],
            unlit.rgb[1],
            unlit.rgb[2],
            lit.rgb[0],
            lit.rgb[1],
            lit.rgb[2],
            factor[0],
            factor[1],
            factor[2],
            lo[0],
            hi[0],
            lo[1],
            hi[1],
            lo[2],
            hi[2],
        );
        assert!(
            unlit.rgb.iter().sum::<f64>() > 20_000.0,
            "{name}: the emitter drew almost nothing ({:?}) -- the station measured nothing",
            unlit.rgb
        );
        assert!(
            lit.touched > 500,
            "{name}: only {} pixels moved",
            lit.touched
        );
        rows.push((name, factor, lo, hi));
    }

    // Retain the calibrated 0.06 slack for card positions/light sets differing from the host
    // bracket. This is not a measured maximum error over every card. A particle drawn without
    // lighting reads 1.000; the far-site ceiling below is the separate calibrated discriminator.
    const SLACK: f64 = 0.06;
    for (name, factor, lo, hi) in &rows {
        for c in 0..3 {
            assert!(
                factor[c] >= lo[c] - SLACK && factor[c] <= hi[c] + SLACK,
                "{name}: channel {c} is drawn at {:.3} of its unlit brightness, but the host-site bracket is {:.3}..{:.3} with 0.06 slack. A zero-luminosity surface retains the default zero emissive term.",
                factor[c], lo[c], hi[c]
            );
        }
    }

    // Non-negative fixed-function diffuse terms put vertex color above ambient, but the
    // measured aggregate image ratio also involves card positions, masking and quantization.
    // The far ceiling is an empirical fixture bound, not an ambient-only upper-bound theorem.
    // The station measures about 0.39 far, and 1.000 with lighting omitted; the near station can
    // reach 1.000 legitimately. About 60 cards are live.
    const CEILING: f64 = 0.65;
    let far = rows.iter().find(|r| r.0 == "far").expect("the far site");
    let near = rows.iter().find(|r| r.0 == "near").expect("the near site");
    for c in 0..3 {
        assert!(
            far.1[c] <= CEILING,
            "far: channel {c} is drawn at {:.3} of its unlit brightness, above the measured ceiling 0.65; the host-site ambient floor is {:.2}. This fixture bound distinguishes the unlit factor of 1.000.",
            far.1[c], far.2[c]
        );
    }

    // Require a relative mean increase at the near site. This does not assert exact saturation
    // or attribute the difference to one particular light independently of the scene light set.
    let mean = |f: &[f64; 3]| (f[0] + f[1] + f[2]) / 3.0;
    assert!(
        mean(&near.1) > mean(&far.1) * 1.5,
        "the same emitter in more light is not brighter: near {:.3} vs far {:.3}",
        mean(&near.1),
        mean(&far.1)
    );
}
