//! The landscape walk passes every land cell of every resident block, and only a cell a building
//! stands in gets ready to draw a building's rooms: the view, its screen polygon and the rooms'
//! lights are worked out there and nowhere else.
//!
//! Fixture: the retail dats and the gpu tier's device.
//!
//! Behaviour: none (the frame's work counted, not what it draws)

#![cfg(gpu)]

use dereth_client_runtime::objects::ObjectStream;
use dereth_client_runtime::scene::SceneConfig;
use dereth_primitives::LocalTime;
use dereth_scene::world_scene::{SceneReads, SceneWrites, WorldScene};

const W: u32 = 640;
const H: u32 = 480;

#[test]
fn only_a_land_cell_a_building_stands_in_gets_ready_to_draw_its_rooms() {
    let _gpu = crate::common::gpu_lock();
    let store = crate::common::dats();
    let mut gpu = crate::common::test_gpu(W, H);
    // Holtburg, its houses all round, the free camera outdoors.
    let cfg = SceneConfig {
        character: false,
        scenery_radius: 2,
        time_of_day: Some(0.5),
        particles: false,
        building_portals: true,
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let mut stream = ObjectStream::new();
    for k in 0..3 {
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            dereth_client_runtime::camera::CameraInput::default(),
            dereth_client_runtime::character::CharacterInput::default(),
            LocalTime(f64::from(k) / 30.0),
            1.0 / 30.0,
        );
        scene.stream(&store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    let buildings = scene.draw.building_shell_levels().len();
    let ready = scene.draw.building_cells_drawn();
    assert!(buildings > 0, "Holtburg's houses are resident");
    assert!(ready > 0, "the walk reached the cells the houses stand in");
    assert!(
        ready as usize <= buildings,
        "{ready} land cells got ready to draw rooms, for {buildings} buildings"
    );
}
