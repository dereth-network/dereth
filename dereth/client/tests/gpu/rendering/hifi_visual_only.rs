//! The optional high-fidelity presentation changes only pixels: the same session, played once
//! with it off and once with it drawing every frame, sends the server the same messages, walks
//! the body to the same places, moves the camera through the same points, offers the same objects
//! to picking, answers the same picks, and sees the selected object drawn on the same frames.
//!
//! The session is the application itself, headless on the `wgpu` device with the fixed-step
//! clock, so its time and its degrade governor follow the frame count and not the frame time. Its
//! network is the socket-free replay endpoint: what the client builds to send is read back from
//! it, and nothing answers.
//!
//! Fixture: the retail dats (`DERETH_TEST_DAT_DIR`), the recorded player's creation, and a
//! hardware `wgpu` device.

#![cfg(all(gpu, feature = "hifi"))]

use std::collections::BTreeSet;

use dereth_client::app::App;
use dereth_client_runtime::actions::movement::action::{MOVE_FORWARD, TURN_LEFT};
use dereth_client_runtime::render_prefs::FidelityPreferences;
use dereth_primitives::{Frame, LocalTime, ObjectId, Viewport};

use crate::common::app::{frames, movement_key};

/// The window the session is played in (the fixture's size).
const SIZE: (u32, u32) = (320, 240);

/// The points of the window a pick is asked at on every frame: the middle (where the body
/// stands), and a grid over the rest.
fn pick_points() -> Vec<(i32, i32)> {
    let mut points = vec![(160, 120), (160, 150), (160, 90)];
    for x in (20..320).step_by(40) {
        for y in (20..240).step_by(40) {
            points.push((x, y));
        }
    }
    points
}

/// A frame's bits: origin then rotation.
fn bits(f: &Frame) -> [u32; 7] {
    [
        f.origin.x.to_bits(),
        f.origin.y.to_bits(),
        f.origin.z.to_bits(),
        f.rotation.w.to_bits(),
        f.rotation.x.to_bits(),
        f.rotation.y.to_bits(),
        f.rotation.z.to_bits(),
    ]
}

/// What one frame of the session decided.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Step {
    /// The body: its cell, and its frame to the bit.
    cell: u32,
    body: [u32; 7],
    /// The chase camera's swept viewpoint (cell and frame) and cell, and the drawn camera.
    viewer_cell: Option<u32>,
    viewer: (u32, [u32; 7]),
    camera: [u32; 7],
    /// The objects offered to picking, the cells drawn, and where each offered object is.
    offered: Vec<u32>,
    drawn_cells: Vec<u32>,
    placed: Vec<(u32, Option<[u32; 7]>)>,
    /// What a pick at each of [`pick_points`] names.
    picks: Vec<u32>,
    /// Whether the selected object was drawn this frame.
    selected_in_view: bool,
    /// Every new message the client built to send this frame: its queue and its bytes.
    sent: Vec<(u16, Vec<u8>)>,
}

/// The client's outgoing messages, read back from the replay endpoint and put back together.
#[derive(Default)]
struct Outbox {
    reassembly: dereth_transport::indicator::Indicator,
    seen: BTreeSet<u64>,
}

impl Outbox {
    /// The messages first built since the last call; a message sent again is not new.
    fn take(&mut self, app: &mut App) -> Vec<(u16, Vec<u8>)> {
        #[allow(clippy::cast_precision_loss)] // a frame count, far below 2^52
        let now = LocalTime(
            app.frames_drawn() as f64 * dereth_client_runtime::platform::clock::HEADLESS_STEP,
        );
        let mut out = Vec::new();
        for (bytes, _) in app
            .replay_network_mut()
            .expect("the replay endpoint")
            .take_outgoing()
        {
            let packet =
                dereth_transport::ParsedPacket::parse(&bytes).expect("the client's packet");
            for blob in
                self.reassembly
                    .check_in_packet(&packet.fragments, packet.header.rec_id, now)
            {
                if self.seen.insert(blob.id.0) {
                    out.push((blob.queue_id, blob.payload));
                }
            }
        }
        out
    }
}

/// What each pick at [`pick_points`] names in the frame just drawn, as the client's own pick
/// would find it.
fn picks(app: &App) -> Vec<u32> {
    let store = crate::common::dats();
    let scene = app.world_scene().expect("the scene");
    let viewport = Viewport {
        x: 0,
        y: 0,
        width: SIZE.0,
        height: SIZE.1,
    };
    pick_points()
        .into_iter()
        .map(|(x, y)| {
            let mut picker = dereth_client_runtime::pick::WorldPicker::new();
            assert!(
                picker.find_object(x, y, viewport),
                "({x}, {y}) is in the view"
            );
            picker
                .draw_no_blit(&store, &scene, app.objects(), SIZE, viewport)
                .map_or(u32::MAX, |id| id.0)
        })
        .collect()
}

/// Play the session with `fidelity`, and what every frame of it decided; and how many of its
/// frames the presentation drew.
#[allow(clippy::too_many_lines)]
fn session(fidelity: FidelityPreferences) -> (Vec<Step>, usize) {
    let _gpu = crate::common::gpu_lock();
    let mut app = crate::common::app::app_with_recorded_body_with(|mut cfg| {
        cfg.renderer = Some(dereth_client_contract::RendererChoice::Wgpu);
        cfg.render.fidelity = fidelity;
        App::new(cfg)
    });
    app.renderer_mut()
        .world_mut()
        .expect("the scene is loaded")
        .cfg
        .render
        .fidelity = fidelity;
    // The network the session talks to: a socket-free endpoint with no one at the other end.
    let mut net = dereth_client_runtime::net::ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "hifi-visual-only",
        "unused",
        0,
    )
    .expect("a socket-free replay endpoint");
    net.session.transport.add_connection(
        0xB,
        0,
        1,
        0xDEAD_BEEF,
        0x1234_5678,
        Some("127.0.0.1:19000".parse().expect("an address")),
    );
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");
    let mut outbox = Outbox::default();
    frames(&mut app, 4);
    let _ = outbox.take(&mut app);
    // Something to keep selected: the first object the frame offered, else the body itself.
    let selected = app
        .renderer()
        .world()
        .and_then(|d| d.drawn_objects())
        .and_then(|o| o.into_iter().next())
        .or_else(|| app.objects().player())
        .expect("an object to select");
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(selected),
        true,
        &mut dereth_client_model::RecordingSink::default(),
    );
    frames(&mut app, 2);
    let _ = outbox.take(&mut app);

    let mut steps = Vec::new();
    let mut composited = 0;
    let mut time = 500_000;
    let mut step = |app: &mut App| {
        app.probe_mut().objects_mut().world.selected_object_in_view = false;
        frames(app, 1);
        let sent = outbox.take(app);
        let ws = app.world_state().expect("the world");
        let body = crate::common::app::body(app);
        let p = body.position();
        let draw = app.renderer().world().expect("the scene");
        let offered: Vec<ObjectId> = draw
            .drawn_objects()
            .unwrap_or_default()
            .into_iter()
            .collect();
        steps.push(Step {
            cell: p.cell.0,
            body: bits(&p.frame),
            viewer_cell: body.camera.viewer_cell.map(|c| c.0),
            viewer: (body.camera.viewer.cell.0, bits(&body.camera.viewer.frame)),
            camera: bits(&ws.camera.frame()),
            placed: offered
                .iter()
                .map(|id| (id.0, ws.server_object_frame(*id).as_ref().map(bits)))
                .collect(),
            offered: offered.iter().map(|o| o.0).collect(),
            drawn_cells: draw.drawn_cells().unwrap_or_default().into_iter().collect(),
            picks: picks(app),
            selected_in_view: app.objects().world.selected_object_in_view,
            sent,
        });
        if app
            .renderer()
            .gpu
            .hifi_report()
            .is_some_and(|r| r.composited)
        {
            composited += 1;
        }
    };
    for (action, held) in [(MOVE_FORWARD, 45), (TURN_LEFT, 20), (MOVE_FORWARD, 30)] {
        movement_key(&mut app, action, true, time);
        for _ in 0..held {
            step(&mut app);
        }
        time += 1_000;
        movement_key(&mut app, action, false, time);
        for _ in 0..5 {
            step(&mut app);
        }
        time += 1_000;
    }
    (steps, composited)
}

/// Behaviour: hifi.visual-only.a-session-plays-identically-with-the-presentation-on
/// A session that walks the body forward, turns and walks again decides exactly the same on
/// every frame with the presentation drawing every frame as with it off: the messages it sends
/// the server, byte for byte; the body's cell, position and heading, the camera's viewpoint and
/// the offered objects' positions, to the bit; the objects offered to picking and the cells
/// drawn; what a pick at each of a grid of points names; and whether the selected object was
/// drawn.
#[test]
fn a_session_walks_turns_and_offers_the_same_objects_with_the_presentation_on() {
    let (off, off_drawn) = session(FidelityPreferences::default());
    let (on, on_drawn) =
        session(FidelityPreferences::parse_switch("Debug=2").expect("the depth view"));
    assert_eq!(off_drawn, 0, "the presentation drew with it off");
    assert!(
        on_drawn + 2 >= on.len(),
        "the presentation drew {on_drawn} of {} frames",
        on.len()
    );
    // What the session must exercise for the comparison to see anything.
    assert!(
        off.first().map(|s| (s.cell, s.body)) != off.last().map(|s| (s.cell, s.body)),
        "the body did not move, so this test cannot see"
    );
    assert!(
        off.iter().any(|s| !s.offered.is_empty()),
        "nothing was offered"
    );
    assert!(
        off.iter().any(|s| s.picks.iter().any(|p| *p != 0)),
        "no pick named anything, so this test cannot see"
    );
    assert!(
        off.iter().any(|s| s.selected_in_view),
        "the selected object was never seen drawn, so this test cannot see"
    );
    let sent: usize = off.iter().map(|s| s.sent.len()).sum();
    let movement = off
        .iter()
        .flat_map(|s| &s.sent)
        .filter(|(_, m)| m.get(..4) == Some(0xF7B1_u32.to_le_bytes().as_slice()))
        .count();
    assert!(
        movement > 0,
        "the session sent no game action ({sent} messages), so this test cannot see"
    );
    assert_eq!(off.len(), on.len());
    for (i, (a, b)) in off.iter().zip(&on).enumerate() {
        assert_eq!(a, b, "frame {i} of the session decided differently");
    }
}

/// Behaviour: hifi.startup.a-device-that-cannot-draw-the-presentation-says-so
/// A player who turns the presentation on while the client runs on a device that cannot draw it
/// is told so once, on the channel the client's own refusals use, and the presentation stays
/// off.
#[test]
fn turning_the_presentation_on_where_it_cannot_draw_tells_the_player_once() {
    let _gpu = crate::common::gpu_lock();
    let mut app = crate::common::app::app_with_recorded_body_with(|mut cfg| {
        cfg.renderer = Some(dereth_client_contract::RendererChoice::Vulkan);
        App::new(cfg)
    });
    // A line the client adds to the scroll is drained from it by the next frame, so each frame's
    // queue is read once.
    let told = |app: &App| {
        app.objects()
            .world
            .scroll
            .pending()
            .iter()
            .filter(|l| {
                l.body == dereth_client_runtime::app::FIDELITY_REFUSED
                    && l.chat_type == dereth_client_model::scroll::LOCAL_ERROR_TYPE
            })
            .count()
    };
    assert_eq!(told(&app), 0);
    app.renderer_mut()
        .world_mut()
        .expect("the scene is loaded")
        .cfg
        .render
        .fidelity = FidelityPreferences::parse_switch("Debug=1").expect("pass-through");
    let added = app.objects().world.scroll.added;
    let mut seen = 0;
    for _ in 0..4 {
        frames(&mut app, 1);
        seen += told(&app);
    }
    assert_eq!(seen, 1, "the player is told once");
    assert_eq!(
        app.objects().world.scroll.added,
        added + 1,
        "and nothing else is said"
    );
    assert!(app.renderer().gpu.hifi_report().is_none());
}
