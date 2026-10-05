//! The corpse loot window: using a corpse from the toolbar sends the recorded use action, the
//! contents answer (`0x0196`) opens the external-container window with its rows in recorded order,
//! late object creates decorate existing rows with icons, a double-click moves an item and a
//! refusal releases its ghost, and the close button, a server close and walking out of the use
//! radius each close the window exactly once. Queued notices never replay into a rebuilt gameplay
//! screen. Every action's bytes are compared with the recorded client message.
//! Fixture: the `long-solo-play` corpus (corpse `0x80000A96`) fed unchanged into a headless
//! gameplay `App` with the retail dats; no server and no fabricated answers.
#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use dereth_client_model::Request;
use dereth_client_net::client_session::{
    testing::{Corpus, CorpusBlob, Direction},
    SessionEvent,
};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Opcode;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::panels::external_container::{ExternalContainerPanel, CLOSE};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use {dereth_client::app::App, dereth_client_runtime::config::Config};

const CORPSE: ObjectId = ObjectId(0x8000_0a96);
const PLAYER: ObjectId = ObjectId(0x5000_000a);
const PAGE: ElementId = ElementId(0x1000_005d);

fn app() -> App {
    let mut app = app_without_ui();
    start_gameplay(&mut app);
    app
}

fn app_without_ui() -> App {
    App::new(Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    })
    .expect("required retail DATs and headless graphics device")
}

fn start_gameplay(app: &mut App) {
    app.start_shell().expect("UI shell");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        assert!(app.frame());
    }
}

fn corpus() -> &'static [CorpusBlob] {
    &Corpus::shared("long-solo-play").blobs
}

fn feed(app: &mut App, row: &CorpusBlob) {
    if row.dir != Direction::ServerToClient {
        return;
    }
    let event = match row.opcode {
        0xf7b0 => SessionEvent::UiEvent {
            opcode: Opcode(u32::from_le_bytes(row.payload[12..16].try_into().unwrap())),
            blob: row.payload[12..].to_vec(),
        },
        0xf746 => SessionEvent::PlayerCreated(ObjectId(u32::from_le_bytes(
            row.payload[4..8].try_into().unwrap(),
        ))),
        _ => SessionEvent::WorldObject {
            opcode: Opcode(row.opcode),
            body: row.payload[4..].to_vec(),
        },
    };
    let now = LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64());
    app.probe_mut().objects_mut().apply_event(&event, now);
    app.apply_hud_events(std::slice::from_ref(&event));
    app.probe_mut()
        .apply_interaction_events(std::slice::from_ref(&event));
}

fn element(app: &App, id: ElementId) -> ElemHandle {
    let shell = app.ui().unwrap();
    let any: &dyn std::any::Any = shell.flow.current().expect("screen");
    let screen = any.downcast_ref::<GamePlayScreen>().expect("gameplay");
    shell
        .ui
        .get_child_recursive(screen.root().unwrap(), id)
        .expect("shipped DAT element")
}

fn drawn(app: &App, mut h: ElemHandle) -> bool {
    let ui = &app.ui().unwrap().ui;
    loop {
        if !ui.node(h).expect("live element").region.flags.visible {
            return false;
        }
        let Some(parent) = ui.parent(h) else {
            return true;
        };
        h = parent;
    }
}

fn click_use(app: &mut App, object: ObjectId) -> Vec<Request> {
    // Initial world selection is setup; the USE action itself is the real DAT toolbar button.
    app.probe_mut().objects_mut().world.set_selected_object(
        Some(object),
        false,
        &mut dereth_client_model::RecordingSink::default(),
    );
    assert!(app.frame());
    assert_eq!(
        app.objects().world.selected,
        Some(object),
        "USE setup selection remains live"
    );
    let button = element(app, dereth_ui_screens::toolbar::target_mode::USE_BUTTON);
    app.ui_mut().unwrap().ui.broadcast_element_message(
        button,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
        0,
    );
    assert!(app.frame());
    app.interaction().last_sent.clone()
}

fn panel(app: &App) -> &ExternalContainerPanel {
    &app.hud().panels.external_container
}

fn row(rows: &[CorpusBlob], index: usize) -> &CorpusBlob {
    rows.iter()
        .find(|r| r.idx == index)
        .expect("recorded station")
}

fn assert_action(requests: &[Request], recorded: &CorpusBlob) {
    assert_eq!(recorded.dir, Direction::ClientToServer);
    assert_eq!(recorded.opcode, 0xf7b1);
    let actions: Vec<_> = requests
        .iter()
        .filter_map(|r| match r {
            Request::UseEvent(m) => Some((0x36_u32, dereth_protocol::write_body(m).unwrap())),
            Request::PutItemInContainer(m) => Some((0x19, dereth_protocol::write_body(m).unwrap())),
            _ => None,
        })
        .collect();
    assert_eq!(
        actions.len(),
        1,
        "recorded station {}: exactly one action: {requests:?}",
        recorded.idx
    );
    let mut bytes = actions[0].0.to_le_bytes().to_vec();
    bytes.extend(&actions[0].1);
    assert_eq!(
        bytes,
        recorded.payload[8..],
        "opcode and complete action body, except transport sequence"
    );
}

fn gesture(
    app: &mut App,
    source: ElemHandle,
    id: dereth_ui::MessageId,
    action: u32,
) -> Vec<Request> {
    app.ui_mut()
        .unwrap()
        .ui
        .broadcast_element_message(source, id, action, 0);
    let mut requests = Vec::new();
    // Keep a second settling frame as a duplicate-action negative control. Target delivery
    // now completes RemainingPanels requests in their producing frame, not the next frame.
    for _ in 0..2 {
        assert!(app.frame());
        requests.extend(app.interaction().last_sent.iter().cloned());
    }
    requests
}

fn pointer_click(app: &mut App, source: ElemHandle, at: u32) -> Vec<Request> {
    // Win32-shaped messages only, into the headless App input manager: no native injection.
    // The second click's timestamp lets the input manager itself classify the double-click.
    let ui = &app.ui().unwrap().ui;
    let rect = ui.screen_clip_box(source);
    assert!(rect.is_valid(), "actual clipped loot row");
    let (x, y) = ((rect.x0 + rect.x1) / 2, (rect.y0 + rect.y1) / 2);
    let hit = ui.hit_test_screen(x, y).expect("real pointer hit");
    assert!(
        hit == source || ui.is_ancestor_of(source, hit),
        "loot row must be hittable, not merely visible"
    );
    let mut pump = dereth_desktop::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let messages = [
        pump.mouse_move_message(f64::from(x), f64::from(y), at),
        pump.mouse_button_message(winit::event::MouseButton::Left, true, at + 10)
            .unwrap(),
        pump.mouse_button_message(winit::event::MouseButton::Left, false, at + 20)
            .unwrap(),
    ];
    for message in messages {
        pump.dispatch(message);
        app.input_manager_mut()
            .expect("real input maps")
            .on_message(message);
    }
    let mut requests = Vec::new();
    for _ in 0..2 {
        assert!(app.frame());
        requests.extend(app.interaction().last_sent.iter().cloned());
    }
    requests
}

/// Behaviour: container.ground.the-contents-reply-fills-the-panel-unless-it-answers-a-pickup
#[test]
fn recorded_corpse_contents_open_an_effectively_visible_dat_window() {
    let mut app = app();
    let rows = corpus();
    for row in rows.iter().filter(|r| r.idx < 6586) {
        feed(&mut app, row);
    }
    assert_eq!(app.objects().world.player, Some(PLAYER));
    assert!(app.frame());
    assert!(
        app.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .take()
            .is_empty(),
        "HUD refill calls must finish before the next fresh selection/input frame"
    );
    assert!(
        !drawn(&app, element(&app, PAGE)),
        "external window starts closed"
    );
    let requests = click_use(&mut app, CORPSE);
    assert_action(&requests, row(&rows, 6586));
    assert_eq!(app.objects().world.ground_object, Some(CORPSE));
    assert!(
        !drawn(&app, element(&app, PAGE)),
        "USE alone is not a contents answer"
    );
    for r in rows.iter().filter(|r| (6587..=6588).contains(&r.idx)) {
        feed(&mut app, r);
    }
    for _ in 0..2 {
        assert!(app.frame());
    }
    assert!(
        drawn(&app, element(&app, PAGE)),
        "0196 must show the real external window AND its ancestors"
    );
    assert_ne!(
        app.objects().world.open_container,
        Some(CORPSE),
        "loot window must not become pickup destination"
    );

    let expected: Vec<_> = (0x8000_0a97..=0x8000_0a9c).rev().map(ObjectId).collect();
    let items = panel(&app)
        .item_list
        .as_ref()
        .expect("actual shipped loot ItemList");
    assert_eq!(
        items
            .slots
            .iter()
            .filter_map(|s| s.item)
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        items
            .slots
            .iter()
            .filter(|s| s.item.is_some())
            .all(|s| s.icon_recipe(&app.ui().unwrap().ui).is_none()),
        "0196 precedes item descriptors"
    );
    let ids_before: Vec<_> = items.slots.iter().map(|s| s.handle).collect();
    for r in rows.iter().filter(|r| (6589..=6594).contains(&r.idx)) {
        feed(&mut app, r);
    }
    assert!(app.frame());
    let items = panel(&app).item_list.as_ref().unwrap();
    assert_eq!(
        items.slots.iter().map(|s| s.handle).collect::<Vec<_>>(),
        ids_before,
        "late create decorates existing rows"
    );
    for slot in items.slots.iter().filter(|s| s.item.is_some()) {
        assert!(
            matches!(
                slot.icon_recipe(&app.ui().unwrap().ui),
                Some(dereth_ui::region::IconRecipe::Object { icon: Some(_), .. })
            ),
            "late F745 must supply actual DAT icon"
        );
        assert!(drawn(&app, slot.handle), "loot row effectively visible");
    }
    for r in rows.iter().filter(|r| (6595..=6609).contains(&r.idx)) {
        feed(&mut app, r);
    }
    assert!(app.frame());

    let loot = ObjectId(0x8000_0a99);
    let slot = panel(&app)
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .find(|s| s.item == Some(loot))
        .unwrap()
        .handle;
    let requests = pointer_click(&mut app, slot, 100_000);
    assert!(!requests
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_))));
    assert_eq!(app.objects().world.selected, Some(loot));
    let requests = pointer_click(&mut app, slot, 100_100);
    assert_action(&requests, row(&rows, 6610));
    assert_eq!(app.objects().world.request_lock.object, Some(loot));
    assert!(app.frame());
    assert!(
        panel(&app)
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .find(|s| s.item == Some(loot))
            .unwrap()
            .waiting
    );
    // Apply only unchanged recorded server messages between request and success.
    for r in rows.iter().filter(|r| (6611..=6618).contains(&r.idx)) {
        feed(&mut app, r);
    }
    assert!(app.frame());
    assert!(app.objects().world.request_lock.is_idle());
    assert!(app.objects().world.is_owned_by_player(loot));
    assert!(!panel(&app).item_list.as_ref().unwrap().is_in_list(loot));
    assert_eq!(
        panel(&app)
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .filter_map(|s| s.item)
            .count(),
        5
    );

    // A second operation actually reaches the server seam; this recorded quest item is refused.
    for r in rows.iter().filter(|r| (6619..=6628).contains(&r.idx)) {
        feed(&mut app, r);
    }
    // App's headless clock is fixed-step. Preserve object use's 200ms throttle rather than
    // clearing last_used or letting host speed choose whether this gesture is accepted.
    for _ in 0..13 {
        assert!(app.frame());
    }
    let refused = ObjectId(0x8000_0a98);
    let slot = panel(&app)
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .find(|s| s.item == Some(refused))
        .unwrap()
        .handle;
    let requests = gesture(&mut app, slot, dereth_ui::msg::element::id::MOUSE_PRESS, 10);
    assert_action(&requests, row(&rows, 6629));
    assert_eq!(app.objects().world.request_lock.object, Some(refused));
    for r in rows.iter().filter(|r| (6630..=6639).contains(&r.idx)) {
        feed(&mut app, r);
    }
    assert!(app.frame());
    assert!(app.objects().world.request_lock.is_idle());
    let refused_slot = panel(&app)
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .find(|s| s.item == Some(refused))
        .unwrap();
    assert!(
        !refused_slot.waiting,
        "failure releases the visible ghost without removing the item"
    );
    assert!(!app.objects().world.is_owned_by_player(refused));

    for r in rows.iter().filter(|r| (6640..=6660).contains(&r.idx)) {
        feed(&mut app, r);
    }

    for _ in 0..13 {
        assert!(app.frame());
    }

    let close = element(&app, CLOSE);
    let requests = gesture(
        &mut app,
        close,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        0,
    );
    assert_action(&requests, row(&rows, 6661));
    assert!(!drawn(&app, element(&app, PAGE)));
    assert_eq!(panel(&app).ground_object, None);
    assert!(panel(&app)
        .item_list
        .as_ref()
        .unwrap()
        .slots
        .iter()
        .all(|s| s.item.is_none()));
    assert_eq!(
        app.objects().world.ground_object,
        Some(CORPSE),
        "local close awaits the server's ground-object change"
    );
    feed(&mut app, row(&rows, 6662));
    feed(&mut app, row(&rows, 6663));
    for _ in 0..2 {
        assert!(app.frame());
        assert!(
            !app.interaction().last_sent.iter().any(|r| matches!(
                r,
                Request::UseEvent(_) | Request::NoLongerViewingContents(_)
            )),
            "server close is not answered again"
        );
    }
    assert_eq!(app.objects().world.ground_object, None);
    app.shutdown();
}

/// A contents notice reaches only the live window subscriber: one delivered before the gameplay
/// screen exists, or queued for a screen that is then rebuilt, never opens the window, while a
/// fresh notice does. The recorded `0x0196` body is redelivered only as a notice stimulus; no
/// pickup or fabricated answer is involved.
#[test]
fn notices_do_not_replay_into_late_or_rebuilt_gameplay_and_fresh_notices_can_reopen() {
    use dereth_client_model::range::RangeHandler;
    let rows = corpus();
    let mut app = app_without_ui();
    for r in rows.iter().filter(|r| r.idx < 6586) {
        feed(&mut app, r);
    }
    // There is intentionally no toolbar subscriber. Seed the same object-use precondition through
    // production model code; this test concerns the subsequent notice's lifetime, not that action.
    app.probe_mut().objects_mut().world.use_object(
        &mut dereth_client_model::RecordingRequests::default(),
        &mut dereth_client_model::RecordingSink::default(),
        CORPSE,
        dereth_client_model::inventory::SplitState::default(),
        dereth_primitives::ServerTime(0.0),
    );
    assert_eq!(app.objects().world.ground_object, Some(CORPSE));
    feed(&mut app, row(&rows, 6588));
    app.probe_mut()
        .objects_mut()
        .world
        .object_range_checks
        .register(
            RangeHandler::Vendor,
            ObjectId(999),
            1.0,
            false,
            false,
            1.0,
            0.0,
            0.0,
        );
    assert!(app.frame()); // no UiShell: drain and discard, no retained history
    assert!(!app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::ExternalContainer, CORPSE));
    assert!(
        app.objects()
            .world
            .object_range_checks
            .is_watching(RangeHandler::Vendor, ObjectId(999)),
        "unrelated handler is not owned by the absent container window"
    );
    start_gameplay(&mut app);
    assert_eq!(panel(&app).ground_object, None);
    assert!(!drawn(&app, element(&app, PAGE)));

    feed(&mut app, row(&rows, 6588));
    assert!(app.frame());
    assert_eq!(panel(&app).ground_object, Some(CORPSE));
    assert!(app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::ExternalContainer, CORPSE));
    assert!(
        drawn(&app, element(&app, PAGE)),
        "fresh notice reaches the new subscriber"
    );
    let old = panel(&app).root.unwrap();
    feed(&mut app, row(&rows, 6588));
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    assert!(app.frame());
    assert_ne!(
        panel(&app).root,
        Some(old),
        "same-mode reconstruction binds fresh handles"
    );
    assert!(!app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::ExternalContainer, CORPSE));
    assert!(app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::Vendor, ObjectId(999)));
    assert_eq!(
        panel(&app).ground_object,
        None,
        "old subscriber's queued notice cannot open its replacement"
    );
    assert!(!drawn(&app, element(&app, PAGE)));
    let host = element(
        &app,
        dereth_ui_screens::screens::gameplay::window::ENV_PANEL,
    );
    let height_before = app
        .ui()
        .unwrap()
        .ui
        .node(element(&app, PAGE))
        .unwrap()
        .region
        .box_
        .height();
    let width_before = app.ui().unwrap().ui.node(host).unwrap().region.box_.width();
    let expected_size = dereth_ui_screens::panels::panel_stack::PanelStack::size_clamps(
        &app.ui().unwrap().ui,
        host,
    )
    .apply(width_before, height_before);
    feed(&mut app, row(&rows, 6588));
    assert!(app.frame());
    assert!(
        drawn(&app, element(&app, PAGE)),
        "same corpse can reopen on a fresh notice"
    );
    assert!(
        app.objects()
            .world
            .object_range_checks
            .is_watching(RangeHandler::ExternalContainer, CORPSE),
        "fresh notice re-registers the replacement subscriber"
    );
    let shell = app.ui().unwrap();
    let root = element(
        &app,
        dereth_ui_screens::screens::gameplay::window::ENV_PANEL,
    );
    assert_eq!(
        shell.ui.node(root).unwrap().region.box_.height(),
        expected_size.1,
        "ENV host receives the page's pre-resize height, with retail min/max clamps"
    );

    // The server closes a window that is still open (distinct from the close-button journey).
    feed(&mut app, row(&rows, 6662));
    for _ in 0..2 {
        assert!(app.frame());
        assert!(!app.interaction().last_sent.iter().any(|r| matches!(
            r,
            Request::UseEvent(_) | Request::NoLongerViewingContents(_)
        )));
    }
    assert!(!drawn(&app, element(&app, PAGE)));
    assert_eq!(panel(&app).ground_object, None);
    app.shutdown();
}

/// Behaviour: inventory.external-container.leaving-use-range-closes-the-window-once
///
/// A corpse descriptor whose omitted use radius defaults to 0.0 still arms a range watch.
/// The App drives the recorded open reply, normal range producer, Interaction journal, Hud consumer
/// and actual window. The window stays open while real bodies coincide, then closes after the
/// local body moves; no close notice is manually supplied here.
#[test]
fn app_range_exit_closes_the_window_once_and_preserves_an_in_range_control() {
    use dereth_client_model::range::RangeHandler;
    let rows = corpus();
    let mut app = app();
    app.load_static_scene(dereth_client_runtime::scene::SceneConfig {
        land_radius: 0,
        scenery_radius: 0,
        particles: false,
        character: true,
        ..Default::default()
    })
    .expect("real DAT scene and player physics body");
    // Copy recorded weenie facts, then express a descriptor that omitted UseRadius on the wire.
    // Descriptor initialization has already materialized that omission as the float 0.0
    // by the time the external-container panel reads the object's use radius.
    let mut recorded = dereth_client_runtime::objects::ObjectStream::new();
    for r in rows
        .iter()
        .filter(|r| r.idx < 6586 && r.dir == Direction::ServerToClient && r.opcode == 0xf745)
    {
        let id = ObjectId(u32::from_le_bytes(r.payload[4..8].try_into().unwrap()));
        if [PLAYER, CORPSE].contains(&id) {
            recorded.apply_event(
                &SessionEvent::WorldObject {
                    opcode: Opcode(r.opcode),
                    body: r.payload[4..].to_vec(),
                },
                LocalTime(0.0),
            );
        }
    }
    for id in [PLAYER, CORPSE] {
        app.probe_mut().objects_mut().world.tables.weenies.insert(
            id,
            recorded
                .world
                .weenie(id)
                .expect("recorded descriptor")
                .clone(),
        );
    }
    app.probe_mut()
        .objects_mut()
        .world
        .weenie_mut(CORPSE)
        .expect("recorded corpse")
        .pwd
        .use_radius = None;
    app.probe_mut().objects_mut().world.player = Some(PLAYER);
    app.apply_hud_events(&[SessionEvent::PlayerCreated(PLAYER)]);

    // Put a real corpse body at the player's position while keeping it outside the parallel
    // server-object handle map. The range geometry lookup must still find its physical body.
    let corpse_position = {
        let mut scene = app.world_scene_mut().expect("the scene");
        let character = scene.character.as_mut().expect("the local physics body");
        let player = character
            .world
            .get(character.handle)
            .expect("the player body");
        let geometry = std::sync::Arc::clone(&player.geometry);
        let position = player.position;
        let handle = character.world.create(CORPSE, geometry, false);
        character.world.force_into_cell(handle, &position);
        *character
            .world
            .get(handle)
            .expect("the corpse body")
            .position()
    };
    assert!(app.objects().physics.handle(CORPSE).is_none());
    assert_action(&click_use(&mut app, CORPSE), row(&rows, 6586));
    feed(&mut app, row(&rows, 6588));
    assert!(app.frame());
    assert!(drawn(&app, element(&app, PAGE)));
    assert!(app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::ExternalContainer, CORPSE));
    let watch = app
        .objects()
        .world
        .object_range_checks
        .live()
        .find(|e| e.handler == RangeHandler::ExternalContainer && e.object == CORPSE)
        .expect("the omitted field armed a watch");
    assert_eq!(
        watch.range, 0.0,
        "the descriptor reset default, not no registration"
    );
    let armed = watch.next_update;
    app.probe_mut()
        .objects_mut()
        .world
        .object_range_checks
        .register(
            RangeHandler::ExternalContainer,
            PLAYER,
            1.0,
            true,
            false,
            1.0,
            0.0,
            armed - 1.0,
        );
    let mut uses = Vec::new();
    // 1-second poll at 30Hz headless fixed-step. Coincident bodies are in the inclusive zero
    // radius, so the first poll must not blanket-close the window.
    for _ in 0..40 {
        assert!(app.frame());
    }
    assert!(
        drawn(&app, element(&app, PAGE)),
        "zero distance remains inside zero radius"
    );

    let mut away = corpse_position;
    away.frame.origin.x += 2.0;
    app.probe_mut()
        .world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(away);

    // Next one-second poll observes the actual movement; include the following UI notice drain.
    for _ in 0..40 {
        assert!(app.frame());
        uses.extend(
            app.interaction()
                .last_sent
                .iter()
                .filter(|r| matches!(r, Request::UseEvent(_)))
                .cloned(),
        );
    }
    assert_action(&uses, row(&rows, 6661));
    assert!(!drawn(&app, element(&app, PAGE)));
    assert_eq!(panel(&app).ground_object, None);
    assert!(!app
        .objects()
        .world
        .object_range_checks
        .is_watching(RangeHandler::ExternalContainer, CORPSE));
    assert!(
        app.objects()
            .world
            .object_range_checks
            .is_watching(RangeHandler::ExternalContainer, PLAYER),
        "positive body-distance control survives"
    );
    app.shutdown();
}
