//! The world picker against the retail dats: aimed straight at a recorded object it returns that
//! object's id, and turned away it returns none, the no-selection result.
//! Fixture: the retail dats' setup and graphics-object geometry and the corpus's own recorded
//! objects, replayed into an `ObjectStream`; no device.
//!
//! Behaviour: none (the picker's geometry against real objects; the selection behaviours it
//! serves are asserted by the selection stations)

use crate::common::client_dir;

use std::collections::BTreeMap;

use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client::pick::{PickScene, WorldPicker};
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::recording::connection_sequence_number;
use dereth_primitives::{Frame, LocalTime, ObjectId, Quat, Vec3};

// ---------------------------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------------------------

/// Fails when the retail dats are not where `$DERETH_TEST_DAT_DIR` says.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

// ---------------------------------------------------------------------------------------------
// 1. The pick sweep, against the retail dats and the corpus's own objects
// ---------------------------------------------------------------------------------------------

/// A [`PickScene`] built by hand: the viewer and one object's frame, which is all the sweep reads.
struct Scene {
    viewer: Frame,
    frames: BTreeMap<ObjectId, Frame>,
}

impl PickScene for Scene {
    fn viewer(&self) -> Frame {
        self.viewer
    }
    fn object_frame(&self, id: ObjectId) -> Option<Frame> {
        self.frames.get(&id).copied()
    }
    fn fov_y_rad(&self, _viewport: (u32, u32)) -> f32 {
        // The default field-of-view preference, expressed by the camera constant.
        dereth_render::camera::DEFAULT_FOV_DEGREES * dereth_render::camera::DEG_TO_RAD
    }
}

/// Replay a capture into a real [`ObjectStream`] — the same three calls `App::frame` makes, minus
/// the renderer — and stop while the world is still populated.
fn replay_populated(session: &str) -> Option<ObjectStream> {
    let records = shared_session(session);
    let csn = connection_sequence_number(records).expect("the capture has a LoginRequest");
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7305, "ac01", "pass", csn).ok()?;
    let mut objects = ObjectStream::new();
    let mut best: Option<usize> = None;
    for (i, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for _ in objects.pump(&mut net, now) {}
        if !objects.is_empty() {
            best = Some(i);
        }
    }
    let stop = best?;

    // Replay through the last record with a nonempty object model, not its maximum population.
    // This avoids later teardown when present without assuming every recording ends in logout.
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7306, "ac01", "pass", csn).ok()?;
    let mut objects = ObjectStream::new();
    for r in records.iter().take(stop + 1) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for _ in objects.pump(&mut net, now) {}
    }
    (!objects.is_empty()).then_some(objects)
}

/// DAT setup/graphics-object geometry for recorded objects, swept by the current picker.
///
/// Each trial places one recorded object in front of the viewer. At least one candidate must
/// return its own id; for that successful candidate, turning away must return zero, matching
/// the no-selection result. Not every candidate is required to intersect the ray.
#[test]
fn a_pick_aimed_at_a_retail_object_returns_its_id_and_aimed_away_returns_none() {
    have_dats();
    let store =
        dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the dats would not open");
    let objects = replay_populated("early-inventory-and-casting")
        .expect("early-inventory-and-casting reaches the world");

    // Pick a candidate the sweep can actually see: an object with a setup, a position and no
    // parent — the same three tests `draw_no_blit` applies.
    let candidates: Vec<ObjectId> = objects
        .presences()
        .filter(|(_, p)| p.setup_id.is_some() && p.position.is_some() && p.parent.is_none())
        .map(|(id, _)| id)
        .collect();
    assert!(
        !candidates.is_empty(),
        "the capture created placeable objects"
    );

    // The back-buffer dimensions used for field of view are separate from the viewport rectangle
    // used to subtract the pick origin and centre its ray. Both are 800x600 here; dereth-client-runtime's
    // pick tests independently cover an offset, smaller viewport and its unsigned edge checks.
    let screen = (800u32, 600u32);
    let viewport = dereth_render::camera::Viewport {
        x: 0,
        y: 0,
        width: screen.0,
        height: screen.1,
    };
    let identity = Quat::new(1.0, 0.0, 0.0, 0.0);
    let mut hits = 0usize;
    let mut tried = 0usize;
    for id in candidates.iter().copied() {
        // Stand 3 m south of the object, level with it, looking north (+y) — the client's
        // convention. Only this object is in the scene, so a hit is unambiguous.
        let scene = Scene {
            viewer: Frame::new(Vec3::new(0.0, -3.0, 0.7), identity),
            frames: [(id, Frame::new(Vec3::new(0.0, 0.0, 0.0), identity))]
                .into_iter()
                .collect(),
        };
        // A one-object stream: `draw_no_blit` walks every presence, so the others are excluded by
        // the scene having no frame for them.
        let mut pick = WorldPicker::new();
        assert!(
            pick.find_object(399, 299, viewport),
            "the centre of the viewport is inside it"
        );
        let found = pick
            .draw_no_blit(&store, &scene, &objects, screen, viewport)
            .expect("a pick was armed");
        tried += 1;
        if found == id {
            hits += 1;
            // ...and the same pick aimed 90 degrees away finds nothing at all.
            let turned = Scene {
                viewer: Frame::new(
                    Vec3::new(0.0, -3.0, 0.7),
                    Quat::new(
                        std::f32::consts::FRAC_1_SQRT_2,
                        0.0,
                        0.0,
                        std::f32::consts::FRAC_1_SQRT_2,
                    ),
                ),
                frames: scene.frames.clone(),
            };
            let mut away = WorldPicker::new();
            assert!(away.find_object(399, 299, viewport));
            assert_eq!(
                away.draw_no_blit(&store, &turned, &objects, screen, viewport),
                Some(ObjectId(0)),
                "turned away, the ray must miss"
            );
            break;
        }
    }
    assert!(
        hits > 0,
        "none of {tried} retail objects was picked when aimed straight at it"
    );
}
