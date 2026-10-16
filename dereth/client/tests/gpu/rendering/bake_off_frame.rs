//! A landscape whose blocks' objects are baked off the frame ends up exactly the landscape that
//! bakes them on it, and while the landscape streams over several frames no block's objects but
//! the viewer's own block's are baked on the frame.
//!
//! With a streaming budget, a block whose objects are to be baked waits while a thread makes its
//! mesh, gathers every placement's triangles and decodes their surfaces' pictures, and the frame
//! only finishes them; and a block waits while another thread decodes the ground's pictures its
//! splat layers need. The block the viewer stands in is built at once, as the window is.
//! Streamed until nothing is left queued, the window must hold the same blocks, objects and
//! triangles, byte for byte, with the same surfaces, as the same window baked on the frame.
//! Fixture: the retail dats' Holtburg window (landblock 0xA9B4).
//!
//! Behaviour: none (an equivalence of two bake paths inside the client, not a retail claim)

#![cfg(gpu)]

use std::time::{Duration, Instant};
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_client_runtime::present::Scene as _;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

fn config(budget: Option<Duration>) -> SceneConfig {
    // Holtburg and the blocks around it: a town's worth of buildings, statics and scenery.
    SceneConfig {
        landblock: 0xA9B4,
        land_radius: 2,
        scenery_radius: 1,
        character: false,
        stream_budget: budget,
        // The ground blended as it is drawn, layer by layer, as the client draws it.
        terrain_splat: true,
        ..SceneConfig::default()
    }
}

#[test]
fn a_landscape_whose_objects_are_baked_off_the_frame_matches_one_baked_on_it() {
    let store = crate::common::dats();
    let mut gpu = crate::common::test_gpu(320, 240);
    let mut streamed = WorldScene::load(&store, &mut gpu, config(Some(Duration::from_millis(4))))
        .expect("the streamed scene loads");
    // A guard against a hang, not a measure: the thread and the mip worker finish in seconds.
    let deadline = Instant::now() + Duration::from_secs(120);
    while streamed.pending_slot_count() > 0 {
        assert!(
            Instant::now() < deadline,
            "the streamed scene never finished"
        );
        streamed.stream(&store, &mut gpu).expect("streams");
        std::thread::sleep(Duration::from_millis(1));
    }
    let (streamed_baked_on_frame, off_frame) = streamed.draw.objects_baked();
    assert!(
        off_frame >= 8,
        "the blocks round the viewer's are baked off the frame: {off_frame}"
    );
    assert!(
        streamed.draw.pictures_decoded_off_frame() > 0,
        "the surfaces' pictures are decoded off the frame"
    );
    let (streamed_on_frame, off_frame) = streamed.draw.splat_pictures_decoded();
    assert!(
        off_frame > 0,
        "the ground's pictures are decoded off the frame"
    );
    let s = streamed.census();
    let digest = streamed.draw.baked_objects_digest();
    streamed.release_textures(&mut gpu);
    drop(streamed);

    let mut direct =
        WorldScene::load(&store, &mut gpu, config(None)).expect("the direct scene loads");
    assert_eq!(
        direct.pending_slot_count(),
        0,
        "an unbudgeted load builds at once"
    );
    let (on_frame, off_frame) = direct.draw.objects_baked();
    assert!(
        on_frame >= 9 && off_frame == 0,
        "baked on the frame: {on_frame}, {off_frame}"
    );
    // The viewer's block's alone (again, if its pictures' mip chains were left to the mip
    // worker), against every block's.
    assert!(
        streamed_baked_on_frame < on_frame,
        "blocks' objects baked on the frame while streaming: {streamed_baked_on_frame} against          {on_frame}"
    );
    let (on_frame, off_frame) = direct.draw.splat_pictures_decoded();
    assert!(
        on_frame > 0 && off_frame == 0,
        "the ground's pictures decoded on the frame: {on_frame}, {off_frame}"
    );
    assert!(
        streamed_on_frame < on_frame,
        "while streaming the frame decodes only the viewer's block's ground pictures:          {streamed_on_frame} against {on_frame} for the whole window"
    );
    let d = direct.census();
    let direct_digest = direct.draw.baked_objects_digest();
    direct.release_textures(&mut gpu);

    // The triangles drawn follow the levels the parts draw at, which the two scenes pick at
    // different moments of the load; the digest holds every level's.
    assert_eq!(
        (
            s.blocks_meshed,
            s.scenery_objects,
            s.buildings,
            s.static_objects
        ),
        (
            d.blocks_meshed,
            d.scenery_objects,
            d.buildings,
            d.static_objects
        ),
        "off the frame (blocks, scenery, buildings, statics) against on it"
    );
    assert!(
        d.scenery_objects + d.buildings + d.static_objects > 0,
        "the window has objects to compare"
    );
    assert_eq!(digest, direct_digest, "the baked objects, byte for byte");
}
