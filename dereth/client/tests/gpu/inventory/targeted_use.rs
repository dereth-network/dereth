//! Targeted item use: using an item arms the targeted mode and retains the item as the source
//! through selection changes, refusals and cancel; acquiring a compatible target sends one
//! `Inventory_UseWithTargetEvent 0x0035` whose bytes equal the recorded message, while an
//! incompatible target, a confirmation dialog or a retained-item target sends nothing; the
//! busy count follows the send and the recorded use-done release.
//! Fixture: the `long-solo-play` corpus (oil `0x80000A6F`, bow `0x800009A4`) applied to an
//! `ObjectStream`; the `app_journey` tests drive a headless gameplay `App` with the retail dats.
use dereth_client::objects::ObjectStream;
use dereth_client_net::client_session::{
    testing::{Corpus, CorpusBlob, Direction},
    SessionEvent,
};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::Opcode;

const PLAYER: ObjectId = ObjectId(0x5000_000a);
const OIL: ObjectId = ObjectId(0x8000_0a6f);
const BOW: ObjectId = ObjectId(0x8000_09a4);

fn recorded_world(before: usize) -> ObjectStream {
    let mut objects = ObjectStream::new();
    objects.world.player = Some(PLAYER);
    let rows = &Corpus::shared("long-solo-play").blobs;
    for row in rows.iter().filter(|r| r.idx < before) {
        if let Some(e) = event(row) {
            objects.apply_event(
                &e,
                LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
            );
        }
    }
    objects
}

// Item use retains this exact source and arms targeted mode 3, not generic mode 1.
/// Behaviour: inventory.targeted-use.using-an-oil-arms-target-mode-without-sending
#[test]
fn recorded_oil_use_arms_the_distinct_targeted_mode_without_sending() {
    let mut objects = recorded_world(4315);
    let mut inter = dereth_client::interaction::Interaction::default();
    inter.queue(
        Vec::new(),
        vec![dereth_ui_screens::view::UiRequest::Use(OIL)],
    );
    assert!(inter
        .run_ui_requests(
            &mut objects.world,
            false,
            dereth_primitives::ServerTime(573.0)
        )
        .is_empty());
    assert_eq!(
        inter.target_mode(),
        dereth_client::interaction::TargetMode::UseTarget
    );
    assert!(inter.pending_requests().is_empty());
    assert_eq!(objects.world.magic.busy_count, 0);
}

// Generic use acts on the clicked object, which may replace Use with UseTarget.
#[test]
fn generic_use_cursor_can_rearm_targeted_use_on_the_clicked_oil() {
    let mut objects = recorded_world(4315);
    objects.world.selected = None;
    let mut inter = dereth_client::interaction::Interaction::default();
    inter.queue(
        Vec::new(),
        vec![dereth_ui_screens::view::UiRequest::SetTargetMode(
            dereth_ui_screens::view::TargetMode::Use,
        )],
    );
    inter.run_ui_requests(
        &mut objects.world,
        false,
        dereth_primitives::ServerTime(572.0),
    );
    inter.wrapper_mouse(
        dereth_client::ui::UiMouseEvent {
            action: 7,
            start: true,
            x: 30,
            y: 30,
            over: None,
        },
        (800, 600),
        true,
    );
    inter.on_world_object_found(
        OIL,
        &mut objects.world,
        dereth_primitives::ServerTime(573.0),
    );
    assert_eq!(
        inter.target_mode(),
        dereth_client::interaction::TargetMode::UseTarget
    );
    assert!(inter.pending_requests().is_empty());
    assert_eq!(objects.world.selected, None);
}

fn arm(
    inter: &mut dereth_client::interaction::Interaction,
    world: &mut dereth_client_model::World,
    item: ObjectId,
    now: f64,
) {
    inter.queue(
        Vec::new(),
        vec![dereth_ui_screens::view::UiRequest::Use(item)],
    );
    assert!(inter
        .run_ui_requests(world, false, dereth_primitives::ServerTime(now))
        .is_empty());
}

// This is the production WorldObjects notice boundary, NOT an assertion about 3D ray picking or
// inventory input. The action bytes and response stream are independent recording oracles.
fn acquire(
    inter: &mut dereth_client::interaction::Interaction,
    world: &mut dereth_client_model::World,
    target: ObjectId,
    now: f64,
) {
    inter.wrapper_mouse(
        dereth_client::ui::UiMouseEvent {
            action: 7,
            start: true,
            x: 30,
            y: 30,
            over: None,
        },
        (800, 600),
        true,
    );
    inter.on_world_object_found(target, world, dereth_primitives::ServerTime(now));
}

fn assert_recorded_request(requests: &[dereth_client_model::Request], index: usize) {
    use dereth_protocol::Message;
    let [dereth_client_model::Request::UseWithTargetEvent(m)] = requests else {
        panic!("exactly one0035, got{requests:?}")
    };
    let rows = &Corpus::shared("long-solo-play").blobs;
    let row = rows.iter().find(|r| r.idx == index).unwrap();
    let mut got = dereth_protocol::items::InventoryUseWithTargetEvent::OPCODE
        .0
        .to_le_bytes()
        .to_vec();
    got.extend(dereth_protocol::write_body(m).expect("encode emitted request"));
    assert_eq!(
        got,
        row.payload[8..],
        "independent retail0035 body, only order stamp excluded"
    );
}

/// Behaviour: inventory.targeted-use.the-source-survives-until-use-done
#[test]
fn retained_source_survives_selection_and_recorded_use_done_allows_a_second_request() {
    let mut objects = recorded_world(4315);
    let mut inter = dereth_client::interaction::Interaction::default();
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    assert_eq!(objects.world.targeting_object, OIL);
    objects.world.selected = Some(PLAYER); // Independent selection change after arming.
    acquire(&mut inter, &mut objects.world, BOW, 573.01); // Less than first-use throttle.
    assert_recorded_request(&inter.take_pending_requests(), 4315);
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.selected, Some(PLAYER));
    assert_eq!(objects.world.magic.busy_count, 1);
    assert!(
        objects.world.weenie(OIL).is_some(),
        "no optimistic deletion"
    );
    assert!(objects.world.weenie(BOW).is_some());

    let rows = &Corpus::shared("long-solo-play").blobs;
    for row in rows.iter().filter(|r| (4316..4363).contains(&r.idx)) {
        if let Some(e) = event(row) {
            objects.apply_event(
                &e,
                LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
            );
            dereth_client::interaction::apply_events(
                &mut inter,
                std::slice::from_ref(&e),
                &mut objects.world,
            );
        }
        if row.idx == 4324 {
            assert_eq!(objects.world.magic.busy_count, 0, "recorded01C7");
        }
    }
    // This test covers the 01C7 busy release and a second independent 0035 only; inventory
    // detach and the 25s deferred destruction after the UI 0024 message are covered elsewhere.
    assert!(
        objects.world.weenie(ObjectId(0x8000_0a74)).is_some(),
        "authoritative new item"
    );
    assert_eq!(objects.world.magic.busy_count, 0);
    let second = ObjectId(0x8000_0a73);
    arm(&mut inter, &mut objects.world, second, 579.0);
    objects.world.selected = Some(PLAYER);
    acquire(
        &mut inter,
        &mut objects.world,
        ObjectId(0x8000_099d),
        579.01,
    );
    assert_recorded_request(&inter.take_pending_requests(), 4363);
    assert_eq!(objects.world.magic.busy_count, 1);
    assert_eq!(objects.world.targeting_object, ObjectId(0));
}

#[test]
fn incompatible_acquisition_consumes_source_without_sending_and_allows_a_fresh_arm() {
    let mut objects = recorded_world(4315);
    let mut inter = dereth_client::interaction::Interaction::default();
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    acquire(&mut inter, &mut objects.world, PLAYER, 573.01);
    assert!(inter.take_pending_requests().is_empty());
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.magic.busy_count, 0);
    arm(&mut inter, &mut objects.world, OIL, 574.0);
    acquire(&mut inter, &mut objects.world, BOW, 574.01);
    assert_recorded_request(&inter.take_pending_requests(), 4315);
}

#[test]
fn first_use_busy_and_throttle_refusals_do_not_replace_the_retained_source() {
    use dereth_client_model::inventory::requests::InventoryRequest;
    let mut objects = recorded_world(4315);
    let mut inter = dereth_client::interaction::Interaction::default();
    let second = ObjectId(0x8000_0a73);
    assert!(
        objects.world.weenie(second).is_some(),
        "throttle is not an unknown-object gate"
    );
    arm(&mut inter, &mut objects.world, OIL, 573.0);
    arm(&mut inter, &mut objects.world, second, 573.1);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "throttle preserves previous target source"
    );
    assert_eq!(
        objects.world.last_used,
        Some(dereth_primitives::ServerTime(573.0))
    );
    objects.world.request_lock.record(
        BOW,
        InventoryRequest::Drop,
        dereth_primitives::ServerTime(573.0),
    );
    arm(&mut inter, &mut objects.world, BOW, 574.0);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "busy refusal preserves previous target source"
    );
    assert!(inter.take_pending_requests().is_empty());
    objects.world.request_lock.clear();
    arm(&mut inter, &mut objects.world, ObjectId(0xffff_ffff), 575.0);
    assert_eq!(
        objects.world.targeting_object, OIL,
        "missing source does not erase retained source"
    );
    arm(&mut inter, &mut objects.world, second, 575.0);
    assert_eq!(
        objects.world.targeting_object, second,
        "positive control after both refusal gates"
    );
}

#[test]
fn generic_use_calls_use_object_on_clicked_door_not_use_with_selected_item() {
    let mut objects = recorded_world(4112);
    let door = ObjectId(0x77f0_3053);
    objects.world.selected = Some(PLAYER);
    let mut inter = dereth_client::interaction::Interaction::default();
    // Source 0 enters the generic toolbar/USE action, rather than using a specific item.
    arm(&mut inter, &mut objects.world, ObjectId(0), 510.0);
    assert_eq!(
        inter.target_mode(),
        dereth_client::interaction::TargetMode::Use
    );
    acquire(&mut inter, &mut objects.world, door, 511.0);
    assert!(matches!(&inter.take_pending_requests()[..],
        [dereth_client_model::Request::UseEvent(m)] if m.object == door));
    assert_eq!(objects.world.targeting_object, ObjectId(0));
    assert_eq!(objects.world.magic.busy_count, 1);
    assert_eq!(objects.world.selected, Some(PLAYER));
}

#[test]
fn targeted_confirmation_gates_and_callback_keep_identity_and_busy_order() {
    use dereth_client_model::inventory::{
        targeted_use::{TargetedUsageConfirmation as C, TargetedUseOutcome as O},
        SplitState,
    };
    use dereth_client_model::{Notice, RecordingRequests, RecordingSink};
    let mut objects = recorded_world(4315);
    let world = &mut objects.world;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    // Branch control built from an otherwise compatible recorded pair: no recording contains an
    // empty-mana-stone or salvage-confirmation gesture.
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x0008_0000;
    world.weenie_mut(OIL).unwrap().pwd.effects = Some(0);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            dereth_primitives::ServerTime(574.0)
        ),
        O::Confirming(C::ManaStone)
    );
    assert_eq!(world.targeting_object, ObjectId(0));
    assert_eq!(
        out.0,
        vec![
            Notice::DisplayString {
                channel: 0x1a,
                text: format!(
                    "Using the {} with the {}",
                    world
                        .weenie(OIL)
                        .unwrap()
                        .object_name(dereth_client_model::weenie::NameType::Appropriate),
                    world
                        .weenie(BOW)
                        .unwrap()
                        .object_name(dereth_client_model::weenie::NameType::Appropriate)
                )
            },
            Notice::TargetedUsageConfirmation {
                source: OIL,
                target: BOW,
                kind: C::ManaStone
            },
        ]
    );
    assert!(req.0.is_empty());
    assert_eq!(
        world.magic.busy_count, 0,
        "dialog/cancel does not claim busy"
    );
    world.selected = Some(PLAYER);
    world.confirm_targeted_usage(
        &mut req,
        &mut out,
        OIL,
        BOW,
        SplitState::default(),
        dereth_primitives::ServerTime(575.0),
    );
    assert_recorded_request(&req.0, 4315);
    assert_eq!(world.magic.busy_count, 1);
    world.use_done(0);
    req.0.clear();
    out.0.clear();
    world.weenie_mut(BOW).unwrap().pwd.bitfield |= 0x0100_0000;
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            dereth_primitives::ServerTime(576.0)
        ),
        O::Retained
    );
    assert_eq!(world.targeting_object, ObjectId(0));
    assert!(req.0.is_empty());
    assert_eq!(world.magic.busy_count, 0);
    assert!(
        matches!(&out.0[..], [Notice::DisplayString { channel: 0x1a, .. }, Notice::DisplayString { channel: 0, text }] if text.contains("\"Retained\""))
    );
    world.weenie_mut(BOW).unwrap().pwd.bitfield &= !0x0100_0000;
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x4000_0000;
    world.player_system.options.set(26, false);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            dereth_primitives::ServerTime(577.0)
        ),
        O::Confirming(C::Salvage)
    );
    assert!(req.0.is_empty());
    world.player_system.options.set(26, true);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            dereth_primitives::ServerTime(578.0)
        ),
        O::Sent
    );
    assert_recorded_request(&req.0, 4315);
}

#[test]
fn accepted_targeted_callback_requires_ids_and_runs_using_item_tail_after_the_send() {
    use dereth_client_model::inventory::{targeted_use::TargetedUseOutcome, SplitState};
    use dereth_client_model::{Notice, RecordingRequests, RecordingSink};
    let mut objects = recorded_world(4315);
    let world = &mut objects.world;
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    world.install_material_names(std::collections::BTreeMap::from([(0x3a, "Bronze".into())]));
    world.weenie_mut(OIL).unwrap().pwd.material_type = Some(0x3a);
    world.weenie_mut(BOW).unwrap().pwd.material_type = Some(0x3a);
    for (source, target) in [(ObjectId(0), BOW), (OIL, ObjectId(0))] {
        world.confirm_targeted_usage(
            &mut req,
            &mut out,
            source,
            target,
            SplitState::default(),
            dereth_primitives::ServerTime(574.0),
        );
        assert!(req.0.is_empty() && out.0.is_empty());
        assert_eq!(world.magic.busy_count, 0);
    }
    // Positive tail: an owned container also carrying a filled mana-stone type. effects 1
    // bypasses the empty-stone dialog; the using-item step's unconditional owned-container tail
    // emits OpenContainedContainer. The normal request still uses the recorded pair's ids.
    world.weenie_mut(OIL).unwrap().pwd.obj_type = 0x0008_0200;
    world.weenie_mut(OIL).unwrap().pwd.items_capacity = Some(1);
    world.weenie_mut(OIL).unwrap().pwd.effects = Some(1);
    world.targeting_object = OIL;
    assert_eq!(
        world.target_acquired(
            &mut req,
            &mut out,
            BOW,
            SplitState::default(),
            dereth_primitives::ServerTime(574.0)
        ),
        TargetedUseOutcome::Sent
    );
    assert_recorded_request(&req.0, 4315);
    assert_eq!(world.magic.busy_count, 1);
    assert_eq!(
        out.0,
        vec![
            Notice::DisplayString {
                channel: 0x1a,
                text: "Using the Bronze Oil of Rendering with the Bronze Training Shortbow".into()
            },
            Notice::OpenContainedContainer(OIL),
        ]
    );
}

fn event(row: &CorpusBlob) -> Option<SessionEvent> {
    if row.dir != Direction::ServerToClient {
        return None;
    }
    Some(match row.opcode {
        0xf7b0 if row.payload[12..16] == 0x13_u32.to_le_bytes() => {
            SessionEvent::PlayerDescription(Box::new(
                dereth_protocol::read_body(&row.payload[16..])
                    .expect("recorded player description"),
            ))
        }
        0xf7b0 => SessionEvent::UiEvent {
            opcode: Opcode(u32::from_le_bytes(row.payload[12..16].try_into().unwrap())),
            blob: row.payload[12..].to_vec(),
        },
        _ if row.queue == dereth_primitives::NetQueue::UiQueue => SessionEvent::UiEvent {
            opcode: Opcode(row.opcode),
            blob: row.payload.clone(),
        },
        0xf746 => SessionEvent::PlayerCreated(ObjectId(u32::from_le_bytes(
            row.payload[4..8].try_into().unwrap(),
        ))),
        _ => SessionEvent::WorldObject {
            opcode: Opcode(row.opcode),
            body: row.payload[4..].to_vec(),
        },
    })
}

#[test]
fn recorded_targeted_pairs_have_independent_source_and_target_descriptors() {
    let mut count = 0;
    for session in ["early-inventory-and-casting", "long-solo-play"] {
        let rows = &Corpus::shared(session).blobs;
        let mut objects = ObjectStream::new();
        for row in rows {
            if let Some(e) = event(&row) {
                objects.apply_event(
                    &e,
                    LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
                );
            } else if row.opcode == 0xf7b1 && row.payload[8..12] == 0x35_u32.to_le_bytes() {
                let source = ObjectId(u32::from_le_bytes(row.payload[12..16].try_into().unwrap()));
                let target = ObjectId(u32::from_le_bytes(row.payload[16..20].try_into().unwrap()));
                for id in [source, target] {
                    let w = objects
                        .world
                        .weenie(id)
                        .expect("recorded target pair exists");
                    eprintln!("{session}/{} {id:?} {:?}, use={:?} targettype={:?}, type={:x}, container={:?}, effect={:?}", row.idx, w.pwd.name, w.pwd.useability, w.pwd.target_type, w.inq_type(), w.pwd.container_id, w.pwd.effects);
                }
                count += 1;
            }
        }
    }
    assert!(count > 0, "the recordings carry no targeted use (0x0035)");
}

#[cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]
mod app_journey {
    use super::*;
    use dereth_client::{
        app::App,
        config::Config,
        pump::{Pump, Win32Message},
    };
    use dereth_client_model::Request;
    use dereth_ui::{ElemHandle, ElementId};
    use dereth_ui_screens::screens::gameplay::GamePlayScreen;

    fn frame(app: &mut App) -> Vec<Request> {
        assert!(app.frame());
        app.interaction().last_sent.clone()
    }

    fn screen(app: &App) -> &GamePlayScreen {
        let any: &dyn std::any::Any = app.ui().unwrap().flow.current().unwrap();
        any.downcast_ref().expect("gameplay")
    }

    fn setup() -> App {
        setup_before(4315, false)
    }

    fn setup_before(before: usize, world: bool) -> App {
        let mut app = App::new(Config {
            headless: true,
            sound: false,
            ui: true,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            ..Config::default()
        })
        .expect("required retail DATs and headless device");
        app.start_shell().expect("shell");
        app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
        if world {
            app.load_static_scene(dereth_client::world::SceneConfig {
                character: false,
                cell_statics: false,
                mesh_collision: false,
                land_radius: 1,
                scenery_radius: 0,
                particles: false,
                ..Default::default()
            })
            .expect("real DAT scene");
        }
        for _ in 0..4 {
            frame(&mut app);
        }
        app.objects_mut().world.player = Some(PLAYER);
        let rows = &Corpus::shared("long-solo-play").blobs;
        for row in rows.iter().filter(|r| r.idx < before) {
            if let Some(e) = event(row) {
                app.objects_mut().apply_event(
                    &e,
                    LocalTime(std::time::Duration::from_micros(row.t_rel_micros).as_secs_f64()),
                );
                app.apply_hud_events(std::slice::from_ref(&e));
                app.apply_interaction_events(std::slice::from_ref(&e));
            }
        }
        let shell = app.ui_mut().unwrap();
        let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().unwrap();
        let screen = any.downcast_mut::<GamePlayScreen>().unwrap();
        let panel = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == ElementId(0x1000_018b))
            .unwrap()
            .panel_id;
        screen.recv_set_panel_visibility(&mut shell.ui, panel, true);
        for _ in 0..3 {
            frame(&mut app);
        }
        dismiss_tutorial_pop_ups(&mut app);
        frame(&mut app);
        app
    }

    /// **The player this replay does not have.**
    ///
    /// `long-solo-play` is the Academy, and its first 4,315 blobs carry
    /// `0x0004 Communication_PopUpString` tutorial prompts. They open one-button message dialogs
    /// on queue **1**; the pop-up handler sets property `0xC3 = 1`, selecting the all-at-once
    /// list.
    ///
    /// In a real session the player dismisses each box as it appears. This harness replays the
    /// whole tutorial in one burst with nobody at the keyboard, so they stack over the middle of
    /// the screen and `hit_test_screen(400, 300)` returns the dialog's panel (`child::PANEL`,
    /// `0x3D`) instead of a world pixel. Dismissing them here is that missing player, and it is
    /// deliberately **not** a suppression of the receiver: if the boxes stopped appearing, the
    /// assertion below would fail.
    fn dismiss_tutorial_pop_ups(app: &mut App) {
        let contexts: Vec<u64> = app
            .ui()
            .unwrap()
            .ui
            .dialogs
            .non_queued()
            .iter()
            .map(|i| i.context)
            .collect();
        assert!(
            !contexts.is_empty(),
            "long-solo-play's tutorial prompts should be on screen by now -- if this is empty the \
             0x0004 receiver has gone away"
        );
        let ui = &mut app.ui_mut().unwrap().ui;
        for context in contexts {
            if let Some(root) = ui.dialogs.close_dialog(context, 0.0) {
                ui.remove_and_delete_root(root);
            }
        }
        assert!(ui.dialogs.non_queued().is_empty(), "every pop-up dismissed");
    }

    fn slot(app: &App, id: ObjectId) -> ElemHandle {
        screen(app)
            .inventory
            .item_list
            .as_ref()
            .unwrap()
            .slots
            .iter()
            .find(|s| s.item == Some(id))
            .expect("actual inventory item")
            .handle
    }

    fn centre(app: &App, h: ElemHandle) -> (i32, i32) {
        let ui = &app.ui().unwrap().ui;
        let mut ancestor = Some(h);
        while let Some(a) = ancestor {
            assert!(
                ui.node(a).unwrap().region.flags.visible,
                "visible ancestor {a:?}"
            );
            ancestor = ui.parent(a);
        }
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid());
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(ui
            .hit_test_screen(at.0, at.1)
            .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)));
        at
    }

    fn click(app: &mut App, pump: &mut Pump, h: ElemHandle, time: u32) -> Vec<Request> {
        let at = centre(app, h);
        let msg = pump.mouse_move_message(f64::from(at.0), f64::from(at.1), time);
        send(app, pump, msg);
        for (down, t) in [(true, time + 1), (false, time + 2)] {
            let msg = pump
                .mouse_button_message(winit::event::MouseButton::Left, down, t)
                .unwrap();
            send(app, pump, msg);
        }
        let mut requests = frame(app);
        requests.extend(frame(app));
        requests
    }

    fn send(app: &mut App, pump: &mut Pump, msg: Win32Message) {
        pump.dispatch(msg);
        app.input_manager_mut().unwrap().on_message(msg);
    }

    fn pump() -> Pump {
        let mut pump = Pump::new();
        pump.state.is_ready = true;
        pump.state.is_active_app = true;
        pump
    }

    fn use_button(app: &App) -> ElemHandle {
        app.ui()
            .unwrap()
            .ui
            .get_child_recursive(
                screen(app).root().unwrap(),
                dereth_ui_screens::toolbar::target_mode::USE_BUTTON,
            )
            .expect("DAT Use button")
    }

    fn assert_cursor(app: &App, key: u32, hot: (i32, i32)) {
        let store = dereth_dat::testing::open_store().unwrap();
        let did =
            dereth_client::assets::enum_did(&store, dereth_client::cursor::UICURSOR_GROUP, key)
                .expect("retail cursor mapper");
        assert_eq!(
            app.current_cursor_did(),
            Some(did),
            "actual App cursor enum{key}; pick={:?} selected={:?} targeting={:?}",
            app.interaction().pick.click_object(),
            app.objects().world.selected,
            app.objects().world.targeting_object,
        );
        assert_eq!(
            app.ui().unwrap().ui.default_cursor,
            Some((did, hot.0, hot.1))
        );
        assert_eq!(app.ui().unwrap().ui.last_cursor, Some((did, hot.0, hot.1)));
        assert_eq!(app.cursor_stats().failures, 0);
    }

    fn escape(app: &mut App, pump: &mut Pump, time: u32) {
        for (down, t) in [(true, time), (false, time + 1)] {
            let m = pump.key_message(down, 0x1b, 1, t);
            send(app, pump, m);
        }
        frame(app);
    }

    #[test]
    fn actual_toolbar_arms_targeted_cursor_and_escape_retains_source_for_a_fresh_operation() {
        use dereth_client::interaction::TargetMode as T;
        let mut app = setup();
        let mut pump = pump();
        let source = slot(&app, OIL);
        assert!(click(&mut app, &mut pump, source, 100_000)
            .iter()
            .all(|r| matches!(r, Request::QueryItemMana(_))));
        let button = use_button(&app);
        assert!(click(&mut app, &mut pump, button, 101_000)
            .iter()
            .all(|r| matches!(r, Request::QueryItemMana(_))));
        assert_eq!(app.interaction().target_mode(), T::UseTarget);
        assert_eq!(app.objects().world.targeting_object, OIL);
        assert_cursor(&app, 0x27, (14, 14));
        // Actual key down/up -> input action -> the cancel action, not a UiRequest injection.
        escape(&mut app, &mut pump, 102_000);
        assert_eq!(app.interaction().target_mode(), T::None);
        assert_eq!(
            app.objects().world.targeting_object,
            OIL,
            "changing target mode preserves the source object"
        );
        assert_cursor(&app, 1, (0, 0));
        assert_eq!(app.objects().world.magic.busy_count, 0);
        // First-use throttle uses the frame timer's current time, not Win32 timestamps or wall sleep.
        // This headless App advances1/30s per production frame; seven frames exceed200ms.
        for _ in 0..7 {
            frame(&mut app);
        }
        click(&mut app, &mut pump, button, 103_000);
        assert_eq!(app.interaction().target_mode(), T::UseTarget);
        assert_cursor(&app, 0x27, (14, 14));
        app.shutdown();
    }

    /// Geometry placement is constructed, NOT a recorded camera/pointer trace. The key,
    /// target identity/qualities and emitted0035 body are independently recorded long-solo-play/4112.
    /// Aluvian DAT meshes supply the actual production polygon pick; no found-id injection.
    fn place_recorded_target_in_view(app: &mut App, target: ObjectId) {
        use dereth_protocol::types::{physicsdesc::flags, ObjDesc, PhysicsDesc};
        let scene = app.world_scene().unwrap();
        let camera = scene.camera.frame();
        let origin = dereth_physics::math::localtoglobal(
            &camera,
            dereth_primitives::Vec3::new(0.0, 10.0, -0.8),
        );
        let block = scene.viewer_block().unwrap();
        // A runtime PWD may contain fields updated independently of its original wire header.
        // Clone the recorded descriptor, whose header/optional fields roundtrip exactly.
        let rows = &Corpus::shared("long-solo-play").blobs;
        let row = rows
            .iter()
            .rev()
            .find(|r| {
                r.idx < 4112
                    && r.dir == Direction::ServerToClient
                    && r.opcode == 0xf745
                    && r.payload[4..8] == target.0.to_le_bytes()
            })
            .expect("recorded target create");
        let recorded = dereth_protocol::read_body_padded::<
            dereth_protocol::objects::ItemCreateObject,
        >(&row.payload[4..])
        .expect("recorded target descriptor");
        let wdesc = recorded.0.wdesc;
        assert_eq!(
            wdesc.obj_type,
            app.objects().world.weenie(target).unwrap().inq_type()
        );
        let instance = app.objects().world.weenie(target).unwrap().instance_seq;
        let body = dereth_protocol::write_body(&dereth_protocol::objects::ItemUpdateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: target,
                objdesc: ObjDesc::default(),
                wdesc,
                physicsdesc: PhysicsDesc {
                    bitfield: flags::POSITION | flags::SETUP,
                    setup_id: Some(0x0200_0001),
                    state: 0,
                    position: Some(dereth_protocol::types::PositionWire {
                        objcell_id: ((block.0 as u32) << 24) | ((block.1 as u32) << 16) | 1,
                        frame: dereth_protocol::types::Frame {
                            origin: origin.into(),
                            orientation: dereth_primitives::Quat::IDENTITY.into(),
                        },
                    }),
                    timestamps: dereth_protocol::types::PhysicsTimestamps {
                        instance,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            },
        ))
        .unwrap();
        dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemUpdateObject>(&body)
            .expect("constructed scene descriptor consumes exactly");
        let previous = app.objects().stats.recreates;
        app.objects_mut().apply_event(
            &SessionEvent::WorldObject {
                opcode: Opcode::ITEM_UPDATE_OBJECT,
                body,
            },
            LocalTime(520.0),
        );
        assert_eq!(app.objects().stats.recreates, previous + 1);
        frame(app);
    }

    #[test]
    fn actual_world_hover_then_click_uses_recorded_key_not_mutable_selection() {
        use dereth_client::interaction::TargetMode as T;
        let key = ObjectId(0x8000_0a6d);
        let door = ObjectId(0x77f0_3053);
        let mut app = setup_before(4112, true);
        let mut pump = pump();
        place_recorded_target_in_view(&mut app, door);
        // Keep the actual world click unobscured by a gameplay panel. This is screen setup,
        // not the target action: selection and Use below still travel through actual controls.
        let set_inventory = |app: &mut App, visible| {
            let shell = app.ui_mut().unwrap();
            let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().unwrap();
            let screen = any.downcast_mut::<GamePlayScreen>().unwrap();
            let panel = screen
                .panels
                .pages
                .iter()
                .find(|p| p.element == ElementId(0x1000_018b))
                .unwrap()
                .panel_id;
            screen.recv_set_panel_visibility(&mut shell.ui, panel, visible);
        };
        set_inventory(&mut app, false);
        frame(&mut app);
        let hit = app.ui().unwrap().ui.hit_test_screen(400, 300);
        let over = hit.map(|h| app.ui().unwrap().ui.node(h).unwrap().element_id());
        assert!(
            dereth_client::interaction::is_world_click(over),
            "world pixel, got {over:?}"
        );
        // This delivery check supplies a real mouse-move event through the frame loop.
        // No button press and no found-id injection may be needed for this fresh polygon pick.
        let selection_before_hover = app.objects().world.selected;
        let m = pump.mouse_move_message(400.0, 300.0, 98_000);
        send(&mut app, &mut pump, m);
        frame(&mut app);
        // **The instrument, before the answer.** The pick has to have *run*: a `requests == 0`
        // means the geometric pick never ran, which otherwise reads like a ray that missed.
        // `pointer_over_game_view` follows the hover's own position; see `app.rs`.
        let picks = app.interaction().pick.stats.requests;
        assert!(
            picks > 0,
            "the move armed a geometric pick ({:?})",
            app.interaction().pick.stats
        );
        assert_eq!(
            app.interaction().pick.click_object().0,
            door,
            "real DAT polygon pick"
        );
        assert_eq!(
            app.objects().world.selected,
            selection_before_hover,
            "hover does not select"
        );
        set_inventory(&mut app, true);
        frame(&mut app);
        let source = slot(&app, key);
        click(&mut app, &mut pump, source, 100_000);
        set_inventory(&mut app, false);
        frame(&mut app);
        let button = use_button(&app);
        click(&mut app, &mut pump, button, 101_000);
        assert_eq!(app.interaction().target_mode(), T::UseTarget);
        assert_eq!(app.objects().world.targeting_object, key);
        // Selection is independently mutable; it must never replace the targeted-use source.
        app.objects_mut().world.set_selected_object(
            Some(PLAYER),
            false,
            &mut dereth_client_model::NullSink,
        );
        let m = pump.mouse_move_message(400.0, 300.0, 102_000);
        send(&mut app, &mut pump, m);
        frame(&mut app);
        assert_eq!(
            app.interaction().pick.click_object().0,
            door,
            "fresh move-only real pick"
        );
        assert_cursor(&app, 0x28, (14, 14));
        app.objects_mut()
            .world
            .weenie_mut(door)
            .unwrap()
            .trade_state = 1;
        frame(&mut app);
        assert_cursor(&app, 0x29, (14, 14));
        app.objects_mut()
            .world
            .weenie_mut(door)
            .unwrap()
            .trade_state = 0;
        frame(&mut app);
        assert_cursor(&app, 0x28, (14, 14));
        let m = pump
            .mouse_button_message(winit::event::MouseButton::Left, true, 103_001)
            .unwrap();
        send(&mut app, &mut pump, m);
        let m = pump
            .mouse_button_message(winit::event::MouseButton::Left, false, 103_002)
            .unwrap();
        send(&mut app, &mut pump, m);
        let requests = frame(&mut app);
        let targeted: Vec<_> = requests
            .into_iter()
            .filter(|r| matches!(r, Request::UseWithTargetEvent(_) | Request::UseEvent(_)))
            .collect();
        assert_recorded_request(&targeted, 4112);
        assert_eq!(app.objects().world.targeting_object, ObjectId(0));
        // Target-mode cleanup consumes the leave flag armed by this press. It runs at the
        // **top of the frame**, before UI element dispatch processes this click, so the click
        // arms cleanup after that frame's cleanup pass. The mode survives the frame the click
        // lands in and is dropped at the top of the next one; `click()` above runs two frames
        // per gesture for this reason, and this hand-rolled world click checks both frames.
        assert_eq!(
            app.interaction().target_mode(),
            T::UseTarget,
            "the click frame arms the leave; it does not perform it"
        );
        frame(&mut app);
        assert_eq!(app.interaction().target_mode(), T::None);
        assert_eq!(app.objects().world.magic.busy_count, 1);
        app.shutdown();
    }

    // Item-list delivery, batching and runtime identity are covered separately from these
    // request assertions.
}
