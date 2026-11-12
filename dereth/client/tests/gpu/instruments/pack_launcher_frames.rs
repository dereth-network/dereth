//! Two skeletons an art pack draws its body for, made side by side before a still camera, the left
//! one handed a longbow as it is made, as a server wields one. With a pack that keeps launchers
//! (`[keep] launchers = true`) the archer is drawn with its own parts and the other with the
//! pack's body. Written to `DERETH_TEST_NAMEPLATE_DUMP` (default: `nameplates/` in cargo's scratch
//! folder for integration tests) as `launchers.png`. It asserts nothing; the claims are the
//! world's and the pack's own tests.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`) and a hardware Vulkan 1.3 device.
//!
//! - `DERETH_ART_PACK`: the packs drawn, a comma list of folders or packed files stacked in that
//!   order (required).

#![cfg(gpu)]
#![cfg(feature = "remaster")]

use std::sync::Arc;

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::objects::ObjectStream;
use dereth_horizon::ui::game::BlipKind;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::{physicsdesc::flags, weeniedesc::header, PhysicsDesc};
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::art_pack::ArtPack;
use dereth_scene::world_scene::{RemasterLight, RemasterPacks, SceneReads, SceneWrites};

use super::nameplate_frames::{
    along, create, place_camera, scene, step, turned, write_png, Subject, H, VIEW, W,
};

const FPS: f64 = 60.0;

const SKELETONS: [Subject; 2] = [
    Subject {
        id: ObjectId(0x8300_0901),
        name: "Skeleton",
        kind: BlipKind::Creature,
        setup: 0x0200_0059,
        mtable: Some(0x0900_0025),
        scale: None,
        at: (-1.3, 8.5),
    },
    Subject {
        id: ObjectId(0x8300_0902),
        name: "Skeleton",
        kind: BlipKind::Creature,
        setup: 0x0200_0059,
        mtable: Some(0x0900_0025),
        scale: None,
        at: (1.3, 8.5),
    },
];

/// The longbow the archer holds, and its setup.
const BOW: ObjectId = ObjectId(0x8300_0903);
const LONGBOW: u32 = 0x0200_0128;

/// The server wielding a longbow on `holder`, in its left hand: a missile weapon, used as one,
/// that shoots arrows.
fn wield_bow(stream: &mut ObjectStream, holder: ObjectId, now: f64) {
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: BOW,
        physicsdesc: PhysicsDesc {
            bitfield: flags::SETUP | flags::PARENT,
            setup_id: Some(LONGBOW),
            parent: Some((holder, 2)),
            timestamps: dereth_protocol::types::PhysicsTimestamps {
                instance: 1,
                ..dereth_protocol::types::PhysicsTimestamps::default()
            },
            ..PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc {
            header: header::COMBAT_USE | header::AMMO_TYPE,
            name: "Longbow".to_owned(),
            wcid: 306,
            obj_type: 0x100,
            combat_use: Some(2),
            ammo_type: Some(1),
            ..Default::default()
        },
        ..Default::default()
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("encode");
    stream.apply_event(
        &SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        LocalTime(now),
    );
    stream.world.update_visible_object_list();
}

/// The skeletons a few seconds after they are made, `pack` drawn.
fn shot(store: &Arc<dereth_dat::RetailDatStore>, pack: Arc<ArtPack>) -> Vec<u8> {
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
    let mut stream = ObjectStream::with_store(Arc::clone(store));
    let mut t = 1.0f64;
    let (made, last) = (120, 300);
    let mut at = None;
    let mut out = Vec::new();
    for i in 0..=last {
        t += 1.0 / FPS;
        if i == made {
            let here = turned(scene.character.as_ref().expect("a body").position(), VIEW);
            for s in &SKELETONS {
                create(&mut stream, s, along(&here, s.at, std::f32::consts::PI), t);
            }
            wield_bow(&mut stream, SKELETONS[0].id, t);
            at = Some(here);
        }
        step(store, &mut gpu, &mut scene, &mut stream, t, |scene| {
            if let Some(at) = at {
                place_camera(scene, &at, (0.0, 2.0, 2.4), 0.0, SKELETONS[0].id);
            }
        });
        if i == last {
            scene
                .reserve_upload_arena(&mut gpu)
                .expect("reserve the arena");
            gpu.begin_frame().expect("begin");
            scene.draw(&mut gpu).expect("draw");
            gpu.end_frame().expect("end");
            out = gpu.capture().expect("capture").to_rgba();
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
fn render_a_skeleton_with_a_bow_beside_one_without() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let paths: Vec<std::path::PathBuf> = std::env::var("DERETH_ART_PACK")
        .expect("DERETH_ART_PACK")
        .split(',')
        .map(std::path::PathBuf::from)
        .collect();
    let pack = Arc::new(ArtPack::open_stack(&paths).expect("the packs open"));
    let rgba = shot(&store, pack);
    let dir = std::env::var("DERETH_TEST_NAMEPLATE_DUMP")
        .unwrap_or_else(|_| format!("{}/nameplates", env!("CARGO_TARGET_TMPDIR")));
    std::fs::create_dir_all(&dir).expect("the folder");
    write_png(&format!("{dir}/launchers.png"), W, H, &rgba);
}
