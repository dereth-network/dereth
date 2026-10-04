//! Shell fixtures and scenarios for viewport.

use super::*;
// =============================================================================================
// viewport.* (dat) -- the two halves that need the shipped geometry and a whole client
//
// The three arithmetic rows are in the `cpu` tier's `shell.rs`, and the reasoning is there.
// =============================================================================================

/// The window, throughout. Every press is given in **window** coordinates.
const VIEW_WINDOW: (u32, u32) = (800, 600);
/// The whole window as a view.
const VIEW_WHOLE: dereth_primitives::viewport::Viewport = dereth_primitives::viewport::Viewport {
    x: 0,
    y: 0,
    width: VIEW_WINDOW.0,
    height: VIEW_WINDOW.1,
};
/// A moved and resized view, deliberately off-centre.
const VIEW_INSET: dereth_primitives::viewport::Viewport = dereth_primitives::viewport::Viewport {
    x: 240,
    y: 90,
    width: 400,
    height: 300,
};
/// That view's own middle, and the window's, which are not the same point.
const VIEW_INSET_MIDDLE: (i32, i32) = (240 + 199, 90 + 149);
const VIEW_WINDOW_MIDDLE: (i32, i32) = (399, 299);

/// The thing to be found, and the shape it is drawn with -- a real body from the shipped data, so
/// the sweep has geometry to sweep.
const VIEW_TARGET: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x8300_0F01);
const VIEW_BODY: dereth_primitives::DataId = dereth_primitives::DataId(0x0200_0001);

/// The viewer and the one thing in front of it -- all the sweep reads, and the reason this needs
/// no device.
struct AView {
    viewer: dereth_primitives::Frame,
    frames: std::collections::BTreeMap<dereth_primitives::ObjectId, dereth_primitives::Frame>,
}

impl dereth_client::pick::PickScene for AView {
    fn viewer(&self) -> dereth_primitives::Frame {
        self.viewer
    }
    fn object_frame(&self, id: dereth_primitives::ObjectId) -> Option<dereth_primitives::Frame> {
        self.frames.get(&id).copied()
    }
    fn fov_y_rad(&self, _window: (u32, u32)) -> f32 {
        dereth_client_contract::camera::DEFAULT_FOV_DEGREES
            * dereth_client_contract::camera::DEG_TO_RAD
    }
}

/// One thing three metres due north of the eye and level with it.
fn one_thing_ahead(
    store: &std::sync::Arc<dereth_dat::RetailDatStore>,
) -> (dereth_client::objects::ObjectStream, AView) {
    let mut objects = dereth_client::objects::ObjectStream::with_store(store.clone());
    let payload = dereth_protocol::objects::ObjectCreatePayload {
        id: VIEW_TARGET,
        objdesc: dereth_protocol::types::ObjDesc::default(),
        physicsdesc: dereth_protocol::types::PhysicsDesc {
            bitfield: dereth_protocol::types::physicsdesc::flags::POSITION
                | dereth_protocol::types::physicsdesc::flags::SETUP,
            setup_id: Some(VIEW_BODY.0),
            position: Some(dereth_protocol::types::PositionWire {
                objcell_id: 0x0001_0001,
                frame: dereth_protocol::types::Frame {
                    origin: dereth_primitives::Vec3::ZERO.into(),
                    orientation: dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0).into(),
                },
            }),
            ..dereth_protocol::types::PhysicsDesc::default()
        },
        wdesc: dereth_protocol::types::PublicWeenieDesc::default(),
    };
    let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemCreateObject(payload))
        .expect("an object create encodes");
    objects.apply_event(
        &dereth_client_net::client_session::SessionEvent::WorldObject {
            opcode: dereth_protocol::Opcode::ITEM_CREATE_OBJECT,
            body,
        },
        dereth_primitives::LocalTime(1.0),
    );
    let identity = dereth_primitives::Quat::new(1.0, 0.0, 0.0, 0.0);
    let view = AView {
        viewer: dereth_primitives::Frame::new(
            dereth_primitives::Vec3::new(0.0, -3.0, 0.7),
            identity,
        ),
        frames: [(
            VIEW_TARGET,
            dereth_primitives::Frame::new(dereth_primitives::Vec3::new(0.0, 0.0, 0.0), identity),
        )]
        .into_iter()
        .collect(),
    };
    (objects, view)
}

/// One whole look: aim at a window point, sweep, and answer what was found.
fn what_is_found_at(
    store: &dereth_dat::RetailDatStore,
    objects: &dereth_client::objects::ObjectStream,
    view: &AView,
    at: (i32, i32),
    viewport: dereth_primitives::viewport::Viewport,
) -> Option<dereth_primitives::ObjectId> {
    let mut pick = dereth_client::pick::WorldPicker::new();
    if !pick.find_object(at.0, at.1, viewport) {
        return None;
    }
    pick.draw_no_blit(store, view, objects, VIEW_WINDOW, viewport)
}

// =============================================================================================
// viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye
// =============================================================================================

/// The rejecting half is the window's middle: a client that ignored the view's corner would find
/// the thing from there, which is exactly what the reported one did.
pub(super) fn a_press_in_the_middle_of_a_moved_view_finds_what_is_ahead_of_the_eye() {
    let mut c = HeadlessClient::new(ClientSpec::retail());
    let store = c
        .dat_store()
        .expect("the retail data files are open")
        .clone();
    let (objects, view) = one_thing_ahead(&store);

    // The control first, so that a client in which nothing is findable at all is caught here
    // rather than read as a pass below.
    let found_through_the_whole_window =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_WHOLE)
            == Some(VIEW_TARGET);

    let found_through_the_moved_view =
        what_is_found_at(&store, &objects, &view, VIEW_INSET_MIDDLE, VIEW_INSET)
            == Some(VIEW_TARGET);

    // ...and the window's middle is not the view's, so aiming there must miss.
    let a_real_difference = VIEW_INSET_MIDDLE != VIEW_WINDOW_MIDDLE;
    let the_windows_middle_misses =
        what_is_found_at(&store, &objects, &view, VIEW_WINDOW_MIDDLE, VIEW_INSET)
            != Some(VIEW_TARGET);

    c.assert_behaviour(
        "viewport.a-press-in-the-middle-of-a-moved-view-finds-what-is-ahead-of-the-eye",
        move |_| {
            found_through_the_whole_window
                && found_through_the_moved_view
                && a_real_difference
                && the_windows_middle_misses
        },
    );
    c.shutdown();
}

// =============================================================================================
// viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not
// =============================================================================================

/// One press on the world, over no part of the interface, through the client's own step.
fn a_press_on_the_world(c: &mut HeadlessClient, x: i32, y: i32) -> (bool, Option<(f32, f32)>) {
    let e = dereth_client::ui::UiMouseEvent {
        action: dereth_ui::focus::action::PRIMARY_CLICK,
        start: true,
        x,
        y,
        over: None,
    };
    let app = c.app_mut();
    let armed = app.probe_mut().interaction_mut().wrapper_mouse(
        e,
        VIEW_WINDOW,
        dereth_client::interaction::is_world_click(e.over),
    );
    (armed, app.interaction().pick.selection_cursor())
}

/// The acceptance: the view is really moved and resized in a running client, a frame runs, and a
/// press is measured against the box the player is now looking through.
pub(super) fn a_really_moved_view_measures_a_press_against_itself() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(2));
    let sbox = element(&c, dereth_ui_screens::hud::world_view::SMART_BOX);

    // The view as it ships: it fills the window from its corner, so the window's number is the
    // view's number and every press in the window is taken.
    let shipped = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(sbox);
    let docked_at_the_corner = (shipped.x0, shipped.y0) == (0, 0);
    let mut the_default_takes_everything = true;
    for (x, y) in [(0, 0), (399, 299), (799, 599), (1, 598)] {
        let (armed, at) = a_press_on_the_world(&mut c, x, y);
        the_default_takes_everything &= armed && at == Some((x as f32, y as f32));
    }
    let nothing_refused_yet = c
        .view()
        .expect_app()
        .interaction()
        .pick
        .stats
        .outside_viewport
        == 0;

    // Now really move and resize it.
    {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        shell.ui.resize_to(sbox, 300, 200);
        shell.ui.move_to(sbox, 120, 80);
    }
    c.tick(1);
    let moved = c
        .view()
        .expect_app()
        .ui()
        .expect("the UI shell is up")
        .ui
        .screen_box(sbox);
    let it_really_moved = moved != shipped && moved.x0 > 0 && moved.y0 > 0;

    // Its own corner is the view's origin.
    let (armed, at) = a_press_on_the_world(&mut c, moved.x0, moved.y0);
    let its_corner_is_the_origin = armed && at == Some((0.0, 0.0));

    // Its own middle, in window coordinates, is the view's middle and not the window's.
    let (cx, cy) = (moved.x0 + moved.width() / 2, moved.y0 + moved.height() / 2);
    let (armed, at) = a_press_on_the_world(&mut c, cx, cy);
    let its_middle_is_the_views =
        armed && at == Some(((moved.width() / 2) as f32, (moved.height() / 2) as f32));

    // ...and the window's own corner is now outside it, and is refused rather than dropped.
    let refusals = c
        .view()
        .expect_app()
        .interaction()
        .pick
        .stats
        .outside_viewport;
    let (armed, _) = a_press_on_the_world(&mut c, 4, 4);
    let outside_is_refused = !armed
        && c.view()
            .expect_app()
            .interaction()
            .pick
            .stats
            .outside_viewport
            == refusals + 1;

    c.assert_behaviour(
        "viewport.a-really-moved-view-measures-a-press-against-itself-and-the-default-is-not",
        move |_| {
            docked_at_the_corner
                && the_default_takes_everything
                && nothing_refused_yet
                && it_really_moved
                && its_corner_is_the_origin
                && its_middle_is_the_views
                && outside_is_refused
        },
    );
    c.shutdown();
}
