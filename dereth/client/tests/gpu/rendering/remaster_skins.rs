//! An art pack's skinned body as the remaster draws it, in place of the parts it names: drawn,
//! playing its own run as the body runs, posed on the device as on the processor, its motion
//! vectors following its limbs, and the traced structures holding it as it is posed, fitted to it
//! each frame without being made again.
//!
//! The body is made in the test ([`mannequin`]): smooth tubes over the human's legs, arms and
//! trunk, run through its parts' places at rest, each vertex on a bone of its own standing where a
//! part rests, blended between two bones across each joint. Its idle and run clips are the
//! human's own stand and run, each bone keyed where its part stands at each of their frames. One
//! leg is magenta, the other cyan, the rest gray; the head, a part it does not name, keeps drawing
//! as it is.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! Behaviour: none (experimental)

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::collections::HashMap;
use std::sync::Arc;

use dereth_client_runtime::character::CharacterInput;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_render::device::{Backend, DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{
    RemasterLight, RemasterPacks, RemasterPost, SceneReads, SceneWrites, WorldScene,
};
use glam::{Mat3, Mat4, Vec3 as V3};
use {dereth_client_runtime::scene::SceneConfig, dereth_primitives::num::math};

const W: u32 = 640;
const H: u32 = 480;
const FPS: f64 = 30.0;

/// The human's setup, which the body is made for and the viewer's body is built on.
pub(crate) const HUMAN: u32 = 0x0200_0001;

/// The viewer's own body, as the traced structures mask it.
const BODY: u8 = 16;

/// Each tube: the parts whose places at rest it runs through, how far it reaches past the last,
/// its radius at its start and at its end, and its primitive (0 and 1 the legs, 2 the rest).
/// Each stretch between two places follows the first of them; the reach follows the last.
const TUBES: [(&[usize], f32, f32, f32, usize); 5] = [
    (&[1, 2, 3, 4], 0.08, 0.085, 0.05, 0),
    (&[5, 6, 7, 8], 0.08, 0.085, 0.05, 1),
    (&[10, 11, 12], 0.16, 0.055, 0.04, 2),
    (&[13, 14, 15], 0.16, 0.055, 0.04, 2),
    (&[0, 9, 16], 0.0, 0.15, 0.12, 2),
];

/// The primitives' colors: magenta, cyan, gray.
const COLORS: [[f32; 4]; 3] = [
    [0.85, 0.05, 0.85, 1.0],
    [0.05, 0.85, 0.85, 1.0],
    [0.6, 0.6, 0.6, 1.0],
];

/// How far either side of a joint a vertex blends between the two parts.
const BAND: f32 = 0.05;

/// Sides a ring, and the length of tube between rings.
const SIDES: usize = 12;
const RING_STEP: f32 = 0.02;

/// A part's place at rest as a matrix, its scale applied.
fn rest_of(part: &dereth_client_runtime::models::ModelPart) -> Mat4 {
    let m = dereth_primitives::frame::l2g(part.frame.rotation).0;
    let o = part.frame.origin;
    Mat4::from_cols(
        glam::Vec4::new(m[0], m[1], m[2], 0.0),
        glam::Vec4::new(m[3], m[4], m[5], 0.0),
        glam::Vec4::new(m[6], m[7], m[8], 0.0),
        glam::Vec4::new(o.x, o.y, o.z, 1.0),
    ) * Mat4::from_scale(V3::new(part.scale.x, part.scale.y, part.scale.z))
}

/// One primitive's vertices: positions, normals, parts and weights, in the setup's axes; and its
/// triangles.
#[derive(Default)]
struct Built {
    positions: Vec<V3>,
    normals: Vec<V3>,
    parts: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// One tube through `places` (the parts' origins at rest), into `out`.
fn tube(out: &mut Built, chain: &[usize], places: &[V3], (reach, r0, r1): (f32, f32, f32)) {
    let mut points: Vec<V3> = chain.iter().map(|&k| places[k]).collect();
    let mut owners: Vec<usize> = chain[..chain.len() - 1].to_vec();
    if reach > 0.0 {
        let n = points.len();
        let dir = (points[n - 1] - points[n - 2]).normalize();
        points.push(points[n - 1] + dir * reach);
        owners.push(chain[n - 1]);
    }
    // Each stretch's start along the tube, and its direction.
    let mut starts = vec![0.0f32];
    for w in points.windows(2) {
        starts.push(starts.last().copied().unwrap_or(0.0) + w[0].distance(w[1]));
    }
    let total = *starts.last().expect("a length");
    let dirs: Vec<V3> = points
        .windows(2)
        .map(|w| (w[1] - w[0]).normalize())
        .collect();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let rings = (total / RING_STEP).ceil().max(1.0) as usize;
    let first = u32::try_from(out.positions.len()).expect("few vertices");
    for k in 0..=rings {
        #[allow(clippy::cast_precision_loss)]
        let s = total * k as f32 / rings as f32;
        let i = (0..dirs.len()).rfind(|&i| starts[i] <= s).unwrap_or(0);
        let centre = points[i] + dirs[i] * (s - starts[i]);
        // Across a joint, between the two stretches' parts and their directions.
        let part = |k: usize| u16::try_from(owners[k]).expect("a part");
        let (mut ps, mut ws, mut dir) = ([part(i), 0, 0, 0], [1.0, 0.0, 0.0, 0.0], dirs[i]);
        for j in 1..dirs.len() {
            let d = s - starts[j];
            if d.abs() < BAND {
                let t = smoothstep((d + BAND) / (2.0 * BAND));
                ps = [part(j - 1), part(j), 0, 0];
                ws = [1.0 - t, t, 0.0, 0.0];
                dir = dirs[j - 1].lerp(dirs[j], t).normalize();
            }
        }
        let reference = if dir.x.abs() < 0.9 { V3::X } else { V3::Y };
        let u = (reference - dir * reference.dot(dir)).normalize();
        let v = dir.cross(u);
        let r = r0 + (r1 - r0) * (s / total);
        for side in 0..SIDES {
            #[allow(clippy::cast_precision_loss)]
            let a = std::f32::consts::TAU * side as f32 / SIDES as f32;
            let n = u * math::cosf(a) + v * math::sinf(a);
            out.positions.push(centre + n * r);
            out.normals.push(n);
            out.parts.push(ps);
            out.weights.push(ws);
        }
    }
    let at = |k: usize, side: usize| first + u32::try_from(k * SIDES + side % SIDES).expect("few");
    for k in 0..rings {
        for side in 0..SIDES {
            let (a, b, c, d) = (
                at(k, side),
                at(k, side + 1),
                at(k + 1, side + 1),
                at(k + 1, side),
            );
            out.indices.extend([a, b, c, a, c, d]);
        }
    }
}

/// The glTF buffer's bytes and its accessors' JSON, as they are added.
#[derive(Default)]
struct Gltf {
    bin: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl Gltf {
    fn add(
        &mut self,
        bytes: &[u8],
        kind: &str,
        component: u32,
        count: usize,
        bounds: &str,
    ) -> usize {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        self.views.push(format!(
            "{{\"buffer\": 0, \"byteOffset\": {}, \"byteLength\": {}}}",
            self.bin.len(),
            bytes.len()
        ));
        self.bin.extend_from_slice(bytes);
        self.accessors.push(format!(
            "{{\"bufferView\": {}, \"componentType\": {component}, \"count\": {count}, \"type\": \"{kind}\"{bounds}}}",
            self.views.len() - 1
        ));
        self.accessors.len() - 1
    }
}

fn floats(v: impl IntoIterator<Item = f32>) -> Vec<u8> {
    v.into_iter().flat_map(f32::to_le_bytes).collect()
}

/// The human's stand and run at peace, which the mannequin's idle and run clips are made from.
const CLIPS: [(&str, u32); 2] = [("idle", 0x0300_0001), ("run", 0x0300_0002)];

/// Each frame of the animation `id`: each part's place in the setup's space.
fn animation_frames(store: &RetailDatStore, id: u32) -> Vec<Vec<Mat4>> {
    use dereth_assets::Decode;
    let id = DataId(id);
    let bytes = store
        .read_typed(dereth_dat::DbType::Anim, id)
        .expect("the animation reads");
    let a = dereth_assets::Animation::decode_payload_in(store.era_of(id), id, &bytes)
        .expect("the animation decodes");
    a.part_frames
        .iter()
        .map(|f| {
            f.frames
                .iter()
                .map(|p| {
                    let m = dereth_primitives::frame::l2g(p.rotation).0;
                    let o = p.origin;
                    Mat4::from_cols(
                        glam::Vec4::new(m[0], m[1], m[2], 0.0),
                        glam::Vec4::new(m[3], m[4], m[5], 0.0),
                        glam::Vec4::new(m[6], m[7], m[8], 0.0),
                        glam::Vec4::new(o.x, o.y, o.z, 1.0),
                    )
                })
                .collect()
        })
        .collect()
}

/// The mannequin's pack, its files by path: a manifest with one `[[skin]]` for the human, and its
/// glTF, made from the human's parts' places at rest in `store` and its stand and run.
#[allow(clippy::too_many_lines)] // the tubes, then the glTF written out
pub(crate) fn mannequin(store: &RetailDatStore) -> HashMap<String, Vec<u8>> {
    let parts = dereth_client_runtime::models::resolve_parts(store, DataId(HUMAN));
    assert!(parts.len() > 16, "the human has its parts");
    let rest: Vec<Mat4> = parts.iter().map(rest_of).collect();
    let places: Vec<V3> = rest.iter().map(|m| m.w_axis.truncate()).collect();
    let mut built: [Built; 3] = Default::default();
    for (chain, reach, r0, r1, p) in TUBES {
        tube(&mut built[p], chain, &places, (reach, r0, r1));
    }
    // The parts named, each a joint, in order; then a bone for each, which the vertices are on.
    let mut followed: Vec<u16> = built
        .iter()
        .flat_map(|b| b.parts.iter().zip(&b.weights))
        .flat_map(|(ps, ws)| {
            ps.iter()
                .zip(ws)
                .filter(|(_, w)| **w > 0.0)
                .map(|(p, _)| *p)
        })
        .collect();
    followed.sort_unstable();
    followed.dedup();
    let joint_of =
        |p: u16| u16::try_from(followed.binary_search(&p).expect("followed")).expect("few");
    // The setup's axes to glTF's: (x, y, z) to (x, z, -y).
    let to_gltf = Mat3::from_cols(V3::X, -V3::Z, V3::Y);
    let g4 = Mat4::from_mat3(to_gltf);
    let mut gltf = Gltf::default();
    let mut prims = Vec::new();
    for (k, b) in built.iter().enumerate() {
        let pos: Vec<V3> = b.positions.iter().map(|p| to_gltf * *p).collect();
        let lo = pos.iter().fold(V3::splat(f32::INFINITY), |a, p| a.min(*p));
        let hi = pos
            .iter()
            .fold(V3::splat(f32::NEG_INFINITY), |a, p| a.max(*p));
        let bounds = format!(
            ", \"min\": [{}, {}, {}], \"max\": [{}, {}, {}]",
            lo.x, lo.y, lo.z, hi.x, hi.y, hi.z
        );
        let n = pos.len();
        let position = gltf.add(
            &floats(pos.iter().flat_map(|p| p.to_array())),
            "VEC3",
            5126,
            n,
            &bounds,
        );
        let normal = gltf.add(
            &floats(b.normals.iter().flat_map(|v| (to_gltf * *v).to_array())),
            "VEC3",
            5126,
            n,
            "",
        );
        let joints: Vec<u8> = b
            .parts
            .iter()
            .zip(&b.weights)
            .flat_map(|(ps, ws)| {
                let mut j = [0u16; 4];
                let bone = u16::try_from(followed.len()).expect("few");
                for c in 0..4 {
                    if ws[c] > 0.0 {
                        j[c] = bone + joint_of(ps[c]);
                    }
                }
                j.into_iter().flat_map(u16::to_le_bytes)
            })
            .collect();
        let joints = gltf.add(&joints, "VEC4", 5123, n, "");
        let weights = gltf.add(
            &floats(b.weights.iter().flatten().copied()),
            "VEC4",
            5126,
            n,
            "",
        );
        let indices: Vec<u8> = b.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
        let indices = gltf.add(&indices, "SCALAR", 5125, b.indices.len(), "");
        prims.push(format!(
            "{{\"attributes\": {{\"POSITION\": {position}, \"NORMAL\": {normal}, \"JOINTS_0\": {joints}, \"WEIGHTS_0\": {weights}}}, \"indices\": {indices}, \"material\": {k}}}"
        ));
    }
    // Each joint's inverse bind matrix: its part's place at rest undone, in glTF's axes; a part's
    // and its bone's alike.
    let binds = floats(
        followed
            .iter()
            .chain(&followed)
            .flat_map(|&p| (g4 * rest[usize::from(p)].inverse() * g4.inverse()).to_cols_array()),
    );
    let binds = gltf.add(&binds, "MAT4", 5126, 2 * followed.len(), "");
    let trs = |m: Mat4| {
        let (_, q, t) = (g4 * m * g4.inverse()).to_scale_rotation_translation();
        (q, t)
    };
    let mut nodes: Vec<String> = followed
        .iter()
        .map(|p| format!("{{\"name\": \"part_{p:02}\"}}"))
        .collect();
    for &p in &followed {
        let (q, t) = trs(rest[usize::from(p)]);
        nodes.push(format!(
            "{{\"name\": \"bone_{p:02}\", \"translation\": [{}, {}, {}], \"rotation\": [{}, {}, {}, {}]}}",
            t.x, t.y, t.z, q.x, q.y, q.z, q.w
        ));
    }
    // The clips: each bone keyed at each frame where its part stands, thirty frames a second.
    let mut animations = Vec::new();
    for (name, id) in CLIPS {
        let frames = animation_frames(store, id);
        #[allow(clippy::cast_precision_loss)]
        let times: Vec<f32> = (0..frames.len()).map(|k| k as f32 / 30.0).collect();
        let last = times.last().copied().unwrap_or(0.0);
        let bounds = format!(", \"min\": [0.0], \"max\": [{last}]");
        let input = gltf.add(
            &floats(times.iter().copied()),
            "SCALAR",
            5126,
            times.len(),
            &bounds,
        );
        let (mut samplers, mut channels) = (Vec::new(), Vec::new());
        for (b, &p) in followed.iter().enumerate() {
            let at_rest = rest[usize::from(p)];
            let posed: Vec<(glam::Quat, V3)> = frames
                .iter()
                .map(|f| trs(f.get(usize::from(p)).copied().unwrap_or(at_rest)))
                .collect();
            let place = gltf.add(
                &floats(posed.iter().flat_map(|(_, t)| t.to_array())),
                "VEC3",
                5126,
                posed.len(),
                "",
            );
            let turn = gltf.add(
                &floats(posed.iter().flat_map(|(q, _)| q.to_array())),
                "VEC4",
                5126,
                posed.len(),
                "",
            );
            let node = followed.len() + b;
            for (output, path) in [(place, "translation"), (turn, "rotation")] {
                channels.push(format!(
                    "{{\"sampler\": {}, \"target\": {{\"node\": {node}, \"path\": \"{path}\"}}}}",
                    samplers.len()
                ));
                samplers.push(format!("{{\"input\": {input}, \"output\": {output}}}"));
            }
        }
        animations.push(format!(
            "{{\"name\": \"{name}\", \"samplers\": [{}], \"channels\": [{}]}}",
            samplers.join(", "),
            channels.join(", ")
        ));
    }
    nodes.push("{\"name\": \"mannequin\", \"mesh\": 0, \"skin\": 0}".to_owned());
    let materials: Vec<String> = COLORS
        .iter()
        .map(|c| {
            format!(
                "{{\"pbrMetallicRoughness\": {{\"baseColorFactor\": [{}, {}, {}, {}], \"metallicFactor\": 0.0, \"roughnessFactor\": 0.7}}, \"doubleSided\": true}}",
                c[0], c[1], c[2], c[3]
            )
        })
        .collect();
    let all: Vec<String> = (0..nodes.len()).map(|i| i.to_string()).collect();
    let joints: Vec<String> = (0..2 * followed.len()).map(|i| i.to_string()).collect();
    let json = format!(
        "{{\"asset\": {{\"version\": \"2.0\"}}, \"scene\": 0, \"scenes\": [{{\"nodes\": [{}]}}], \
         \"nodes\": [{}], \"skins\": [{{\"joints\": [{}], \"inverseBindMatrices\": {binds}}}], \
         \"meshes\": [{{\"primitives\": [{}]}}], \"materials\": [{}], \"animations\": [{}], \
         \"buffers\": [{{\"uri\": \"body.bin\", \"byteLength\": {}}}], \
         \"bufferViews\": [{}], \"accessors\": [{}]}}",
        all.join(", "),
        nodes.join(", "),
        joints.join(", "),
        prims.join(", "),
        materials.join(", "),
        animations.join(", "),
        gltf.bin.len(),
        gltf.views.join(", "),
        gltf.accessors.join(", "),
    );
    let manifest = format!(
        "[pack]\nname = \"Mannequin\"\nformat = 1\n\
         [[skin]]\nsetup = \"{HUMAN:#010X}\"\nfile = \"mannequin/body.gltf\"\n\
         clips = {{ idle = \"idle\", run = \"run\" }}\n\
         author = \"dereth tests\"\ntool = \"generated\"\nlicense = \"CC0-1.0\"\n"
    );
    let mut files = HashMap::new();
    files.insert("manifest.toml".to_owned(), manifest.into_bytes());
    files.insert("mannequin/body.gltf".to_owned(), json.into_bytes());
    files.insert("mannequin/body.bin".to_owned(), gltf.bin);
    files
}

/// The pack of `files`, opened from memory.
pub(crate) fn in_memory(files: &HashMap<String, Vec<u8>>) -> Arc<ArtPack> {
    let manifest = String::from_utf8(files["manifest.toml"].clone()).expect("text");
    let pack = ArtPack::open(&manifest, &mut |path| {
        files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: missing"))
    })
    .expect("the pack opens");
    assert!(pack.problems.is_empty(), "{:?}", pack.problems);
    assert_eq!(pack.skins.len(), 1, "the mannequin reads");
    Arc::new(pack)
}

/// How a run is drawn.
#[derive(Clone, Copy)]
struct Run {
    /// The pack's bodies drawn.
    bodies: bool,
    /// Posed on the device (or on the processor).
    device: bool,
    /// The anti-aliasing on, the motion picture read, and the bodies' last places given to it.
    motion: Option<bool>,
    /// The acceleration structures kept, and the viewer's body's rays checked every `STEP`th
    /// pixel.
    traced: bool,
}

/// Every 4th pixel a side is checked for the traced structures.
const STEP: u32 = 4;

/// One moment of a run: the frame, and when asked, its motion picture, its rays and what its
/// acceleration structures held.
struct Moment {
    rgba: Vec<u8>,
    motion: Option<dereth_scene::world_scene::RemasterPostPicture>,
    rays: Option<Vec<(f32, u32)>>,
    acceleration: Option<dereth_scene::world_scene::RemasterAcceleration>,
}

/// Frames after the run starts that are kept: a stride and more, every frame.
const KEPT: std::ops::Range<usize> = 20..44;

/// The viewer's body, standing two seconds, then running, seen from its side by a camera that
/// follows it; the frames of [`KEPT`].
#[allow(clippy::too_many_lines)] // one run, set up, stepped and drawn
fn run(store: &Arc<RetailDatStore>, pack: &Arc<ArtPack>, how: Run) -> Option<Vec<Moment>> {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        debug: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(Backend::Vulkan, None, &cfg).expect("a Vulkan device");
    assert_eq!(
        gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        gpu.adapter_name()
    );
    let region = dereth_world_data::landblock::load_region(store).expect("the region decodes");
    let scene_cfg = SceneConfig {
        time_of_day: Some(0.42),
        particles: false,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(store, &mut gpu, scene_cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    scene.draw.set_remaster_pack(Some(Arc::clone(pack)), None);
    let packs = RemasterPacks {
        characters: how.bodies,
        ..RemasterPacks::default()
    };
    scene.draw.set_remaster_packs(packs, &mut gpu);
    scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    if let Some(given) = how.motion {
        scene.draw.set_remaster_post(
            RemasterPost {
                taa: true,
                ..RemasterPost::default()
            },
            &mut gpu,
        );
        scene.draw.set_remaster_skin_motion(given, &mut gpu);
    }
    scene.draw.set_remaster_skin_gpu(how.device, &mut gpu);
    if how.traced {
        if !scene.draw.remaster_traces_rays(&mut gpu) {
            eprintln!("{}: no ray queries", gpu.adapter_name());
            return None;
        }
        scene.draw.set_remaster_acceleration(true, &mut gpu);
    }
    let mut stream = dereth_client_runtime::objects::ObjectStream::new();
    let mut input = CharacterInput::default();
    let mut t = 1.0f64;
    #[allow(clippy::cast_possible_truncation)]
    let dt = (1.0 / FPS) as f32;
    let settle = 60;
    let mut out = Vec::new();
    for i in 0..settle + KEPT.end {
        t += 1.0 / FPS;
        if i == settle {
            input.forward = true;
            input.run = true;
        }
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(Default::default(), input, LocalTime(t), dt);
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            Default::default(),
            LocalTime(t),
            1.0 / FPS,
        );
        // From the body's side, at its hips' height.
        if let Some(c) = scene.character.as_ref() {
            let f = c.render_frame();
            let d = c.drawn_offset();
            let at = Vec3::new(f.origin.x + d.x, f.origin.y + d.y, f.origin.z + d.z);
            let m = dereth_primitives::frame::l2g(f.rotation).0;
            let len = (m[3] * m[3] + m[4] * m[4]).sqrt().max(1e-6);
            let (fx, fy) = (m[3] / len, m[4] / len);
            // The forward turned a quarter about the upright axis.
            let (dx, dy) = (-fy, fx);
            let (dist, height) = (2.6, 0.8);
            let eye = Vec3::new(at.x + dx * dist, at.y + dy * dist, at.z + height);
            scene.camera.position = eye;
            scene.camera.yaw = math::atan2f(-(at.x - eye.x), at.y - eye.y);
            scene.camera.pitch = 0.0;
        }
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
        if i >= settle && KEPT.contains(&(i - settle)) {
            let rgba = gpu.capture().expect("capture").to_rgba();
            let motion = how
                .motion
                .and_then(|_| scene.draw.remaster_post_picture(&mut gpu));
            let rays = how.traced.then(|| {
                scene
                    .draw
                    .remaster_acceleration_check(&mut gpu, STEP, BODY)
                    .expect("the structures were built")
            });
            let acceleration = how
                .traced
                .then(|| scene.draw.remaster_acceleration(&mut gpu))
                .flatten();
            out.push(Moment {
                rgba,
                motion,
                rays,
                acceleration,
            });
        }
    }
    scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    scene.release_textures(&mut gpu);
    drop(scene);
    drop(gpu);
    let errors = dereth_render::vulkan::take_validation_errors();
    assert!(errors.is_empty(), "the validation layer: {errors:#?}");
    Some(out)
}

/// Which leg, if either, a color is: 0 the magenta, 1 the cyan.
fn leg_of(r: f32, g: f32, b: f32) -> Option<usize> {
    let hi = r.max(g).max(b);
    if hi <= 0.0 {
        return None;
    }
    let (r, g, b) = (r / hi, g / hi, b / hi);
    if r > 0.7 && b > 0.7 && g < 0.35 {
        Some(0)
    } else if g > 0.7 && b > 0.7 && r < 0.35 {
        Some(1)
    } else {
        None
    }
}

/// Each pixel's leg, by its color in `rgba`.
fn legs(rgba: &[u8]) -> Vec<Option<usize>> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            if p.iter().take(3).all(|c| *c < 40) {
                return None;
            }
            leg_of(f32::from(p[0]), f32::from(p[1]), f32::from(p[2]))
        })
        .collect()
}

/// How many pixels show `leg`, and their mean column.
#[allow(clippy::cast_precision_loss)]
fn count_and_column(legs: &[Option<usize>], leg: usize) -> (usize, f32) {
    let (n, sum) = legs
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == Some(leg))
        .fold((0usize, 0.0f32), |(n, s), (i, _)| {
            (n + 1, s + (i % W as usize) as f32)
        });
    (n, if n > 0 { sum / n as f32 } else { 0.0 })
}

/// **A pack's skinned body draws in place of the parts it names, and plays its own run**: running,
/// the mannequin's legs show at every kept frame, and seen from the side each leg passes the
/// other (the magenta leg's mean column is more than 8 pixels ahead of the cyan's at one frame
/// and behind it at another); with the pack's bodies off, its colors are not drawn at all.
#[test]
fn a_packs_skinned_body_draws_in_place_of_the_parts_and_plays_its_own_run() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = in_memory(&mannequin(&store));
    let on = Run {
        bodies: true,
        device: true,
        motion: None,
        traced: false,
    };
    let drawn = run(&store, &pack, on).expect("drawn");
    let mut apart: Vec<f32> = Vec::new();
    for (k, m) in drawn.iter().enumerate() {
        let legs = legs(&m.rgba);
        let (a, ca) = count_and_column(&legs, 0);
        let (b, cb) = count_and_column(&legs, 1);
        eprintln!("frame {k}: magenta {a} at {ca:.1}, cyan {b} at {cb:.1}");
        assert!(a > 150 && b > 150, "frame {k}: both legs drawn: {a}, {b}");
        apart.push(ca - cb);
    }
    let (lo, hi) = apart
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), d| {
            (l.min(*d), h.max(*d))
        });
    assert!(
        lo < -8.0 && hi > 8.0,
        "each leg passes the other: {apart:?}"
    );
    let off = run(
        &store,
        &pack,
        Run {
            bodies: false,
            ..on
        },
    )
    .expect("drawn");
    let shown: usize = off
        .iter()
        .map(|m| legs(&m.rgba).iter().filter(|l| l.is_some()).count())
        .sum();
    assert!(
        shown < 50,
        "the parts drawn, not the body: {shown} pixels of its colors"
    );
}

/// **Posed on the device, a body is drawn as posed on the processor**: at every kept frame of the
/// run, both draw the legs, and fewer than 40 pixels of the frame differ by more than 24 in any
/// channel.
#[test]
fn a_body_posed_on_the_device_is_drawn_as_posed_on_the_processor() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = in_memory(&mannequin(&store));
    let device = Run {
        bodies: true,
        device: true,
        motion: None,
        traced: false,
    };
    let a = run(&store, &pack, device).expect("drawn");
    let b = run(
        &store,
        &pack,
        Run {
            device: false,
            ..device
        },
    )
    .expect("drawn");
    for (k, (a, b)) in a.iter().zip(&b).enumerate() {
        let (la, lb) = (legs(&a.rgba), legs(&b.rgba));
        let drawn = |l: &[Option<usize>]| l.iter().filter(|l| l.is_some()).count();
        let differ = a
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.rgba.as_chunks::<4>().0)
            .filter(|(p, q)| p.iter().zip(*q).any(|(x, y)| x.abs_diff(*y) > 24))
            .count();
        eprintln!(
            "frame {k}: legs {} and {}, {differ} pixels differ",
            drawn(&la),
            drawn(&lb)
        );
        assert!(
            drawn(&la) > 300 && drawn(&lb) > 300,
            "frame {k}: legs drawn"
        );
        assert!(differ < 40, "frame {k}: {differ} pixels differ");
    }
}

/// The mean length, in pixels, of the motion drawn at the pixels of `pic` showing a leg.
#[allow(clippy::cast_precision_loss)]
fn leg_motion(pic: &dereth_scene::world_scene::RemasterPostPicture) -> (usize, f32) {
    let (wf, hf) = (pic.width as f32, pic.height as f32);
    let mut n = 0usize;
    let mut sum = 0.0f32;
    for (i, (c, m)) in pic.scene.iter().zip(&pic.motion).enumerate() {
        if m[3] < 0.5 || leg_of(c[0], c[1], c[2]).is_none() {
            continue;
        }
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        let (x, y) = ((i % pic.width) as f32 + 0.5, (i / pic.width) as f32 + 0.5);
        let ndc = [
            x / wf * 2.0 - 1.0 - pic.jitter[0],
            1.0 - y / hf * 2.0 - pic.jitter[1],
        ];
        let dx = (m[0] - ndc[0]) * wf * 0.5;
        let dy = (m[1] - ndc[1]) * hf * 0.5;
        n += 1;
        sum += (dx * dx + dy * dy).sqrt();
    }
    (n, if n > 0 { sum / n as f32 } else { 0.0 })
}

/// **A skinned body's motion vectors follow its limbs**: running, with the camera following the
/// body, its legs' pixels move on average more than 1.5 pixels a frame further, and more than half
/// again as far, when the motion pass is given the vertices' last places than when it moves the
/// body only as the part it is drawn through moves.
#[test]
fn a_skinned_bodys_motion_vectors_follow_its_limbs() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = in_memory(&mannequin(&store));
    let given = Run {
        bodies: true,
        device: true,
        motion: Some(true),
        traced: false,
    };
    let mean = |moments: &[Moment]| {
        let all: Vec<(usize, f32)> = moments
            .iter()
            .map(|m| leg_motion(m.motion.as_ref().expect("a motion picture")))
            .collect();
        assert!(all.iter().all(|(n, _)| *n > 100), "legs drawn: {all:?}");
        #[allow(clippy::cast_precision_loss)]
        let mean = all.iter().map(|(_, l)| l).sum::<f32>() / all.len() as f32;
        mean
    };
    let on = mean(&run(&store, &pack, given).expect("drawn"));
    let off = mean(
        &run(
            &store,
            &pack,
            Run {
                motion: Some(false),
                ..given
            },
        )
        .expect("drawn"),
    );
    eprintln!("legs' motion: given {on:.2} px, withheld {off:.2} px a frame");
    assert!(
        on > off + 1.5 && on > off * 1.5,
        "given {on}, withheld {off}"
    );
}

/// Of the checked pixels showing either leg in `rgba`, how many there are and how many have
/// their ray from the eye (`rays`) meet the viewer's body.
fn met(rgba: &[u8], rays: &[(f32, u32)]) -> (usize, usize) {
    let legs = legs(rgba);
    let cols = W.div_ceil(STEP) as usize;
    let (mut shown, mut met) = (0usize, 0usize);
    for (r, (hit, _)) in rays.iter().enumerate() {
        let (x, y) = ((r % cols) * STEP as usize, (r / cols) * STEP as usize);
        if x >= W as usize || y >= H as usize || legs[y * W as usize + x].is_none() {
            continue;
        }
        shown += 1;
        if *hit >= 0.0 {
            met += 1;
        }
    }
    (shown, met)
}

/// **The traced structures hold a skinned body as it is posed that frame**: running with the
/// structures kept, the checked pixels showing either leg have their ray from the eye meet the
/// viewer's body. Posed on the processor or on the device, the structures hold the frame's own
/// pose, not the last frame's: at least 90% of the pixels showing a leg in the frame meet it at
/// every kept frame.
#[test]
fn the_traced_structures_hold_a_skinned_body_as_it_is_posed() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = in_memory(&mannequin(&store));
    let traced = Run {
        bodies: true,
        device: false,
        motion: None,
        traced: true,
    };
    for device in [false, true] {
        let Some(moments) = run(&store, &pack, Run { device, ..traced }) else {
            return;
        };
        for (k, w) in moments.windows(2).enumerate() {
            let rays = w[1].rays.as_ref().expect("rays");
            let now = met(&w[1].rgba, rays);
            let before = met(&w[0].rgba, rays);
            eprintln!(
                "device {device}, frame {}: this frame's legs {now:?}, the last's {before:?}",
                k + 1
            );
            let (shown, met) = now;
            assert!(shown > 20, "frame {}: legs drawn", k + 1);
            assert!(
                met * 100 >= shown * 90,
                "device {device}, frame {}: {met} of {shown}",
                k + 1
            );
        }
    }
}

/// **A skinned body's traced structure is fitted to it each frame, not made again**: running with
/// the structures kept, each kept frame refits one structure (the viewer's body, the only one
/// posed) and makes none.
#[test]
fn a_skinned_bodys_traced_structure_is_refitted_each_frame_not_made_again() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = in_memory(&mannequin(&store));
    let Some(moments) = run(
        &store,
        &pack,
        Run {
            bodies: true,
            device: true,
            motion: None,
            traced: true,
        },
    ) else {
        return;
    };
    for (k, w) in moments.windows(2).enumerate() {
        let (a, b) = (
            w[0].acceleration.expect("the structures' counts"),
            w[1].acceleration.expect("the structures' counts"),
        );
        eprintln!("frame {}: made {} refits {}", k + 1, b.made, b.refits);
        assert_eq!(b.refits, a.refits + 1, "frame {}: one refit", k + 1);
        assert_eq!(b.made, a.made, "frame {}: nothing made", k + 1);
    }
}
