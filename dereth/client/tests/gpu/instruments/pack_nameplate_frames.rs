//! The Horizon interface's names over bodies an art pack draws in place of their parts, the
//! remaster on: a row of a virindi, a person, an olthoi and a lifestone standing before a still
//! camera, the virindi selected so its marker stands over its name. Written to
//! `DERETH_TEST_NAMEPLATE_DUMP` (default: `nameplates/` in cargo's scratch folder for integration
//! tests) as `pack-names.png`. It asserts nothing; the claims are the world's own tests.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_ART_PACK`: the packs drawn, a comma list of folders or packed files stacked in that
//!   order (required).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_runtime::objects::ObjectStream;
use dereth_horizon::ui::game::BlipKind;
use dereth_primitives::ObjectId;
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks, SceneReads, SceneWrites};

use super::nameplate_frames::{
    along, create, nameplates, place_camera, scene, step, turned, write_png, Interface, Subject, H,
    VIEW, W,
};

const FPS: f64 = 60.0;

const ROW: [Subject; 4] = [
    Subject {
        id: ObjectId(0x8300_0601),
        name: "Aluvian",
        kind: BlipKind::Player,
        setup: 0x0200_0001,
        mtable: Some(0x0900_0001),
        scale: None,
        at: (0.9, 9.0),
    },
    Subject {
        id: ObjectId(0x8300_0602),
        name: "Virindi Master",
        kind: BlipKind::Creature,
        setup: 0x0200_0041,
        mtable: Some(0x0900_0028),
        scale: None,
        at: (-0.8, 9.0),
    },
    Subject {
        id: ObjectId(0x8300_0603),
        name: "Olthoi Soldier",
        kind: BlipKind::Creature,
        setup: 0x0200_0AAA,
        mtable: Some(0x0900_0002),
        scale: None,
        at: (2.4, 9.5),
    },
    Subject {
        id: ObjectId(0x8300_0604),
        name: "Life Stone",
        kind: BlipKind::Item,
        setup: 0x0200_02EE,
        mtable: None,
        scale: None,
        at: (6.4, 11.0),
    },
];

/// The row with its names over it, a few seconds after it is made, `pack` drawn.
fn row(store: &Arc<dereth_dat::RetailDatStore>, pack: Arc<ArtPack>) -> Vec<u8> {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        remaster: true,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new_on(crate::common::remaster_backend(), None, &cfg).expect("a device");
    let mut scene = scene(store, &mut gpu);
    scene.draw.set_remaster_pack(Some(pack), None);
    scene
        .draw
        .set_remaster_packs(RemasterPacks::all(), &mut gpu);
    scene
        .draw
        .set_remaster(true, store, &mut gpu)
        .expect("the remaster starts");
    scene
        .draw
        .set_remaster_light(RemasterLight::Pixel, &mut gpu);
    let mut stream = ObjectStream::new();
    let mut ui = Interface::new();
    let mut t = 1.0f64;
    let (made, shot) = (120, 360);
    let mut at = None;
    let subjects: Vec<&Subject> = ROW.iter().collect();
    let mut out = Vec::new();
    for i in 0..=shot {
        t += 1.0 / FPS;
        // The interface's frame, from the world as the last frame left it; the virindi selected.
        let mut plates = nameplates(&scene, &subjects);
        for p in &mut plates {
            p.selected = p.id == ROW[1].id;
        }
        ui.frame(plates);
        if i == made {
            let here = turned(scene.character.as_ref().expect("a body").position(), VIEW);
            for s in &ROW {
                create(&mut stream, s, along(&here, s.at, std::f32::consts::PI), t);
            }
            at = Some(here);
        }
        step(store, &mut gpu, &mut scene, &mut stream, t, |scene| {
            if let Some(at) = at {
                place_camera(scene, &at, (0.5, 2.0, 2.6), 0.08, ROW[1].id);
            }
        });
        ui.follow(&scene);
        if i == shot {
            scene
                .reserve_upload_arena(&mut gpu)
                .expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
            out = gpu.capture().expect("capture").to_rgba();
            for s in &ROW {
                let feet = scene.objects.get(&s.id).map_or(0.0, |o| o.frame.origin.z);
                let own = scene.name_top(s.id).map(|t| t.z - feet);
                let at = scene
                    .target_top(s.id, (W, H))
                    .map(|t| (t.0.round(), t.1.round()));
                println!(
                    "{}: the files' name height {own:?}, its name stands on {at:?}",
                    s.name
                );
            }
            ui.over(&mut out);
        }
    }
    scene
        .draw
        .set_remaster(false, store, &mut gpu)
        .expect("the remaster stops");
    scene.release_textures(&mut gpu);
    out
}

#[test]
#[ignore = "instrument: writes a PNG; run with DERETH_ART_PACK set and --ignored"]
fn render_names_over_a_packs_bodies() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let paths: Vec<std::path::PathBuf> = std::env::var("DERETH_ART_PACK")
        .expect("DERETH_ART_PACK")
        .split(',')
        .map(std::path::PathBuf::from)
        .collect();
    let pack = Arc::new(ArtPack::open_stack(&paths).expect("the packs open"));
    let rgba = row(&store, pack);
    let dir = std::env::var("DERETH_TEST_NAMEPLATE_DUMP")
        .unwrap_or_else(|_| format!("{}/nameplates", env!("CARGO_TARGET_TMPDIR")));
    std::fs::create_dir_all(&dir).expect("the folder");
    write_png(&format!("{dir}/pack-names.png"), W, H, &rgba);
}
