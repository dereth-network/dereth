//! An object the server describes as see-through is drawn that see-through from the moment it is
//! made: every part of it.
//!
//! Fixture: the retail dats and a body in the world the runtime simulates with no device, out of
//! Holtburg. Two drudges (setup `0x020007DD`) are created beside it by encoded `0xF745
//! Item_CreateObject`s through the object stream, one whose description gives a translucency of
//! 0.6 and one whose description gives none.

use std::sync::Arc;

use crate::common::sim::SimWorld;

use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::{LandblockId, LocalTime, ObjectId, Quat, Vec3};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::physicsdesc::flags;
use dereth_protocol::types::{PhysicsDesc, PublicWeenieDesc};
use dereth_protocol::Message;
use dereth_world_data::landblock::DEFAULT_LANDBLOCK;

const DRUDGE: u32 = 0x0200_07DD;
const GHOST: ObjectId = ObjectId(0x7000_6057);
const SOLID: ObjectId = ObjectId(0x7000_6058);
const STEP: f64 = 1.0 / 30.0;

fn create(stream: &mut ObjectStream, id: ObjectId, x: f32, z: f32, translucency: Option<f32>) {
    let payload = ObjectCreatePayload {
        id,
        objdesc: Default::default(),
        physicsdesc: PhysicsDesc {
            bitfield: flags::POSITION
                | flags::SETUP
                | if translucency.is_some() {
                    flags::TRANSLUCENCY
                } else {
                    0
                },
            setup_id: Some(DRUDGE),
            translucency,
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: LandblockId(DEFAULT_LANDBLOCK).cell(1).0,
                frame: dereth_protocol::types::Frame {
                    origin: Vec3::new(x, 20.0, z).into(),
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
            body: dereth_protocol::write_body(&ItemCreateObject(payload)).expect("encodes"),
        },
        LocalTime(0.0),
    );
}

/// Each part's translucency as it is drawn, 0 for a part drawn solid.
fn parts(scene: &SimWorld, id: ObjectId) -> Vec<f32> {
    let o = scene
        .ws()
        .objects
        .get(&id)
        .expect("the drudge is in the scene");
    let d = o.sim.driver.borrow();
    d.part_array
        .parts
        .iter()
        .map(|p| p.material.map_or(0.0, |m| m.translucency))
        .collect()
}

/// Behaviour: rendering.translucency.an-object-is-drawn-as-see-through-as-its-description-says
#[test]
fn a_drudge_described_as_see_through_has_every_part_drawn_see_through() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let mut scene = SimWorld::load(&store, dereth_client_runtime::scene::SceneConfig::default());
    let mut stream = ObjectStream::with_store(Arc::clone(&store));
    let land = Arc::clone(scene.ws().character.as_ref().expect("a body").land());
    let ground = |x: f32| {
        land.ground_height(LandblockId(DEFAULT_LANDBLOCK), x, 20.0)
            .unwrap_or(0.0)
    };
    create(&mut stream, GHOST, 20.0, ground(20.0), Some(0.6));
    create(&mut stream, SOLID, 24.0, ground(24.0), None);
    let mut t = 0.0;
    for _ in 0..10 {
        t += STEP;
        scene.sync_objects(&mut stream).expect("object dispatch");
        #[allow(clippy::cast_possible_truncation)] // one frame's step
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            CharacterInput::default(),
            LocalTime(t),
            STEP as f32,
        );
    }
    let ghost = parts(&scene, GHOST);
    assert!(!ghost.is_empty(), "the drudge has parts");
    assert!(
        ghost.iter().all(|t| (t - 0.6).abs() < 1e-6),
        "every part of the see-through drudge is drawn at 0.6: {ghost:?}"
    );
    let solid = parts(&scene, SOLID);
    assert!(
        solid.iter().all(|t| *t == 0.0),
        "the drudge described with none is drawn solid: {solid:?}"
    );
}
