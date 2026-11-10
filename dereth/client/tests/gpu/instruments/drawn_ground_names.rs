//! A person standing in the grass at Holtburg, on the ground the remaster draws, with the name the
//! interface stands over him and the ring it lays under him when he is selected: the frame as the
//! client draws it, the world first, then the ring, then the interface over both.
//!
//! The place is the town's collector's, in the grass beside the road, where the drawn ground lies
//! off retail's; the camera stands low, a few meters off, so the grass stands between it and him.
//!
//! - `DERETH_ART_PACK`: an art pack the remaster draws (none without it).
//! - `DERETH_REMASTER_CAPTURE_DIR`: where the pictures go (none are written without it).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_contract::overlay::GroundMarker;
use dereth_client_runtime::present::Presentation;
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_client_runtime::report::VisualReport;
use dereth_client_shell::gpu::Renderer;
use dereth_dat::RetailDatStore;
use dereth_horizon::art::Art;
use dereth_horizon::draw::{DrawList, Overlay};
use dereth_primitives::ObjectId;
use dereth_render::pso::{Blend, PipelineKey, SurfaceContext};
use dereth_render::surface::{surface_type, Surface};
use dereth_render::vertex::VertexFormat;
use dereth_render::ZFunc;
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterPacks, SceneReads, SceneWrites};

use super::nameplate_frames::{self, H, W};
use super::stations::{self, report_station, Shot};

/// The person in the grass.
pub(crate) const PERSON: ObjectId = ObjectId(0x8300_0790);
/// Where he stands in Holtburg's block, meters east and north.
const SPOT: (f32, f32) = (87.27, 156.12);
/// The camera, low in the grass north of him, looking at him across it toward the town, at dusk.
const PLACE: &str = "dereth report 1; camera 0xA9B4 [85.3 165.6 67.9] yaw -2.934 pitch -0.12 \
                     cell 0xA9B40027; time 0.757; clock 306486183.56536025; weather clear; draw \
                     distance 3";

/// The place on a renderer of its own with the remaster and its grass on, drawing
/// `pack` when one is given, and the person standing in the grass, settled on the ground.
pub(crate) fn open(
    store: &Arc<RetailDatStore>,
    pack: Option<&std::path::Path>,
) -> (Renderer, Shot) {
    let backend = crate::common::remaster_backend();
    let mut r = Renderer::new_on_for(backend, None, W, H, true)
        .unwrap_or_else(|e| panic!("{backend:?}: no device opened: {e}"));
    assert_eq!(
        r.gpu.adapter_kind(),
        dereth_render::device::AdapterKind::Hardware,
        "{} is a software rasterizer",
        r.gpu.adapter_name()
    );
    let report = VisualReport::parse(PLACE).expect("the place's report reads");
    let mut station = report_station(&report, Some(3));
    station.weather = false;
    let mut shot = Shot::open(store, &mut r.gpu, &station);
    shot.apply_report(&report);
    shot.scene.draw.cfg.render.fidelity =
        FidelityPreferences::parse_switch("Remaster=1,RemasterGrass=1,RemasterFlowers=1")
            .expect("the switch reads");
    shot.poll(&mut r.gpu);
    if let Some(path) = pack {
        let opened = Arc::new(ArtPack::open_path(path).expect("the pack opens"));
        let folder = path.is_dir().then(|| path.to_path_buf());
        shot.scene.draw.set_remaster_pack(Some(opened), folder);
        shot.scene
            .draw
            .set_remaster_packs(RemasterPacks::all(), &mut r.gpu);
    }
    assert!(shot.scene.draw.remaster_on(), "the remaster is on");
    shot.create_object(
        PERSON.0,
        (0x0200_0001, Some(0x0900_0001)),
        0xA9B4,
        SPOT,
        200.0,
    );
    shot.step_held(&mut r.gpu);
    shot.scene.wait_for_remaster_ground();
    for _ in 0..30 {
        shot.step(&mut r.gpu);
    }
    (r, shot)
}

/// The state the ring is laid with, as the client lays it: the interface's picture blended over
/// the world, hidden where the world stands in front of it, writing no depth.
pub(crate) fn ring_key() -> PipelineKey {
    let surface = Surface {
        r#type: surface_type::BASE1_IMAGE,
        ..Surface::default()
    };
    let ctx = SurfaceContext {
        vertex_format: VertexFormat::XyzDiffuseTex1,
        ..SurfaceContext::default()
    };
    let (mut key, _) = PipelineKey::from_surface(&surface, ctx);
    key.alpha_blend = true;
    key.src_blend = Blend::SrcAlpha;
    key.dst_blend = Blend::InvSrcAlpha;
    key.z_func = ZFunc::LessEqual;
    key.z_write = false;
    key
}

/// The world drawn as it stands, `ring` laid under its object (with the ring's state, or over
/// everything when `over_all`), and `list` over it all, as BGRA; drawn a few times so the
/// remaster's frames that gather over time have gathered.
pub(crate) fn frame(
    r: &mut Renderer,
    shot: &mut Shot,
    art: &Art,
    list: &DrawList,
    ring: Option<(&GroundMarker, bool)>,
) -> Vec<u8> {
    let mut overlay = Overlay::default();
    overlay.compose(r, art, list, false, &[]);
    let mut bgra = Vec::new();
    for _ in 0..6 {
        shot.scene
            .reserve_upload_arena(&mut r.gpu)
            .expect("reserve the arena");
        Presentation::start_frame(r).expect("the frame starts");
        shot.scene.draw(&mut r.gpu).expect("the world is drawn");
        if let Some((marker, over_all)) = ring {
            let slot = r
                .overlay_slot(marker.texture)
                .expect("the ring is uploaded");
            let mut key = ring_key();
            if over_all {
                key.z_func = ZFunc::Always;
            }
            shot.scene
                .draw
                .draw_ground_marker(&shot.scene.world, &mut r.gpu, marker, slot, &key)
                .expect("the ring is laid");
        }
        overlay.draw(r).expect("the interface is drawn");
        Presentation::end_frame(r).expect("the frame ends");
        bgra = r.capture_bgra().expect("the frame is read back").2;
    }
    overlay.release(r);
    bgra
}

fn rgba(bgra: &[u8]) -> Vec<u8> {
    let mut out = bgra.to_vec();
    for p in out.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    out
}

/// The person in the grass with his name over him and the ring under him, as the client draws
/// them; the picture written under `DERETH_REMASTER_CAPTURE_DIR`, named for the pack.
#[test]
#[ignore = "instrument: draws a person's name and ring on the drawn ground; run with --ignored"]
fn draw_a_name_and_ring_in_the_grass() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let pack = std::env::var("DERETH_ART_PACK")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(std::path::PathBuf::from);
    let (mut r, mut shot) = open(&store, pack.as_deref());
    let mut ui = nameplate_frames::Interface::new();
    let person = shot.scene.objects.get(&PERSON).expect("the person stands");
    let origin = person.frame.origin;
    println!(
        "the person at {origin:?}: drawn {:.3} m above where he stands; the drawn ground under \
         him {:?}; his name's point {:?}",
        shot.scene.remaster_shift_at(origin),
        shot.scene.remaster_ground_at(origin.x, origin.y),
        shot.scene.target_top(PERSON, (W, H)),
    );
    let subject = nameplate_frames::Subject {
        id: PERSON,
        name: "Collector",
        kind: dereth_horizon::ui::game::BlipKind::Npc,
        setup: 0x0200_0001,
        mtable: Some(0x0900_0001),
        scale: None,
        at: (0.0, 0.0),
    };
    ui.frame(nameplate_frames::nameplates(&shot.scene, &[&subject]));
    ui.follow(&shot.scene);
    let art = Arc::clone(&ui.art);
    let picture = dereth_horizon::ring::picture(&art);
    Presentation::overlay_upload(&mut r, dereth_horizon::ring::RING_TEXTURE, &picture)
        .expect("the ring uploads");
    let marker =
        dereth_horizon::ring::marker(Some((PERSON, dereth_horizon::ui::game::Relation::Npc)), 0.0)
            .expect("a ring");
    let bgra = frame(&mut r, &mut shot, &art, &ui.list, Some((&marker, false)));
    if let Some(dir) = std::env::var("DERETH_REMASTER_CAPTURE_DIR")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        let name = pack.as_ref().and_then(|p| p.file_stem()).map_or_else(
            || "no-pack".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let dir = std::path::PathBuf::from(dir);
        std::fs::create_dir_all(&dir).expect("the folder");
        stations::write_png(&dir.join(format!("{name}.png")), W, H, &rgba(&bgra));
    }
    shot.scene
        .draw
        .set_remaster(false, &store, &mut r.gpu)
        .expect("the remaster stops");
    shot.scene.release_textures(&mut r.gpu);
}
